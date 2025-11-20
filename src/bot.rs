use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_read_progress::TokioAsyncReadProgressExt;
use dashmap::{DashMap, DashSet};
use futures::TryStreamExt;
use grammers_client::{
    Client, InputMessage, Update, button, reply_markup,
    types::{CallbackQuery, Chat, Message, User},
};
use log::{error, info, warn};
use reqwest::Url;
use scopeguard::defer;
use stream_cancel::{Trigger, Valved};
use tokio::sync::Mutex;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::command::{Command, parse_command};

const HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/110.0.0.0 Safari/537.36";
const MAX_FILE_SIZE_BYTES: usize = 2 * 1024 * 1024 * 1024;
const PROGRESS_UPDATE_INTERVAL_SECS: u64 = 3;
const DEFAULT_FILENAME: &str = "file.bin";
const CALLBACK_CANCEL: &[u8] = b"cancel";

#[derive(Debug)]
pub struct Bot {
    client: Client,
    me: User,
    http: reqwest::Client,
    locks: Arc<DashSet<i64>>,
    started_by: Arc<DashMap<i64, i64>>,
    triggers: Arc<DashMap<i64, Trigger>>,
}

impl Bot {
    pub async fn new(client: Client) -> Result<Arc<Self>> {
        let me = client.get_me().await?;
        let http_client = create_http_client()?;

        Ok(Arc::new(Self {
            client,
            me,
            http: http_client,
            locks: Arc::new(DashSet::new()),
            started_by: Arc::new(DashMap::new()),
            triggers: Arc::new(DashMap::new()),
        }))
    }

    pub async fn run(self: Arc<Self>) {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Received Ctrl+C, exiting");
                    break;
                }

                Ok(update) = self.client.next_update() => {
                    let self_ = self.clone();

                    tokio::spawn(async move {
                        if let Err(err) = self_.handle_update(update).await {
                            error!("Error handling update: {}", err);
                        }
                    });
                }
            }
        }
    }

    async fn handle_update(&self, update: Update) -> Result<()> {
        match update {
            Update::NewMessage(msg) => self.handle_message(msg).await,
            Update::CallbackQuery(query) => self.handle_callback(query).await,
            _ => Ok(()),
        }
    }

    async fn handle_message(&self, msg: Message) -> Result<()> {
        if !is_supported_chat(&msg) {
            return Ok(());
        }

        if let Some(command) = parse_command(msg.text()) {
            return self.process_command(msg, command).await;
        }

        if is_private_chat(&msg) {
            if let Ok(url) = Url::parse(msg.text()) {
                return self.handle_url(msg, url).await;
            }
        }

        Ok(())
    }

    async fn process_command(&self, msg: Message, command: Command) -> Result<()> {
        if !self.is_command_for_this_bot(&command) {
            return Ok(());
        }

        if should_ignore_start_in_group(&msg, &command) {
            return Ok(());
        }

        info!("Received command: {:?}", command);
        match command.name.as_str() {
            "start" => self.handle_start(msg).await,
            "upload" => self.handle_upload(msg, command).await,
            _ => Ok(()),
        }
    }

    fn is_command_for_this_bot(&self, command: &Command) -> bool {
        if let Some(via) = &command.via {
            let my_username = self.me.username().unwrap_or_default().to_lowercase();
            if via.to_lowercase() != my_username {
                warn!("Ignoring command for unknown bot: {}", via);
                return false;
            }
        }
        true
    }

    async fn handle_start(&self, msg: Message) -> Result<()> {
        msg.reply(InputMessage::html(
            "📁 <b>Hi! Need a file uploaded? Just send the link!</b>\n\
            In groups, use <code>/upload &lt;url&gt;</code>\n\
            \n\
            🌟 <b>Features:</b>\n\
            \u{1488} Free & fast\n\
            \u{1488} <a href=\"https://github.com/ndiuky/url-uploader\">Open source</a>\n\
            \u{1488} Uploads files up to 2GB\n\
            \u{1488} Redirect-friendly",
        ))
        .await?;
        Ok(())
    }

    async fn handle_upload(&self, msg: Message, cmd: Command) -> Result<()> {
        let url = match cmd.arg {
            Some(url) => url,
            None => {
                msg.reply("Please specify a URL").await?;
                return Ok(());
            }
        };

        let url = match Url::parse(&url) {
            Ok(url) => url,
            Err(err) => {
                msg.reply(format!("Invalid URL: {}", err)).await?;
                return Ok(());
            }
        };

        self.handle_url(msg, url).await
    }

    async fn handle_url(&self, msg: Message, url: Url) -> Result<()> {
        let sender = match msg.sender() {
            Some(sender) => sender,
            None => return Ok(()),
        };

        let chat_id = msg.chat().id();
        if !self.try_lock_chat(chat_id, &msg).await? {
            return Ok(());
        }
        self.started_by.insert(chat_id, sender.id());

        defer! {
            info!("Unlocking chat {}", chat_id);
            self.locks.remove(&chat_id);
            self.started_by.remove(&chat_id);
        };

        info!("Downloading file from {}", url);
        let response = self.http.get(url).send().await?;

        let file_info = extract_file_info(&response)?;
        info!(
            "File {} ({} bytes, video: {})",
            file_info.name, file_info.size, file_info.is_video
        );

        if let Err(error_msg) = validate_file_size(file_info.size) {
            msg.reply(error_msg).await?;
            return Ok(());
        }

        let stream = create_cancellable_stream(response, chat_id, &self.triggers);
        defer! {
            self.triggers.remove(&chat_id);
        };

        let reply_markup = create_cancel_button();
        let status = Arc::new(Mutex::new(
            msg.reply(
                InputMessage::html(format!(
                    "🚀 Starting upload of <code>{}</code>...",
                    file_info.name
                ))
                .reply_markup(reply_markup.as_ref()),
            )
            .await?,
        ));

        let mut stream = create_progress_stream(
            stream,
            file_info.size,
            file_info.name.clone(),
            status.clone(),
            reply_markup.clone(),
        );

        let start_time = chrono::Utc::now();
        let file = self
            .client
            .upload_stream(&mut stream, file_info.size, file_info.name.clone())
            .await?;

        let elapsed = chrono::Utc::now() - start_time;
        info!(
            "Uploaded file {} ({} bytes) in {}",
            file_info.name, file_info.size, elapsed
        );

        self.send_uploaded_file(&msg, file, file_info.is_video, elapsed)
            .await?;
        status.lock().await.delete().await?;

        Ok(())
    }

    async fn try_lock_chat(&self, chat_id: i64, msg: &Message) -> Result<bool> {
        info!("Locking chat {}", chat_id);
        let locked = self.locks.insert(chat_id);
        if !locked {
            msg.reply("✋ Whoa, slow down! There's already an active upload in this chat.")
                .await?;
        }
        Ok(locked)
    }

    async fn send_uploaded_file(
        &self,
        msg: &Message,
        file: grammers_client::types::media::Uploaded,
        is_video: bool,
        elapsed: chrono::Duration,
    ) -> Result<()> {
        let mut input_msg = InputMessage::html(format!(
            "Uploaded in <b>{:.2} secs</b>",
            elapsed.num_milliseconds() as f64 / 1000.0
        ));
        input_msg = input_msg.document(file);

        if is_video {
            input_msg = input_msg.attribute(grammers_client::types::Attribute::Video {
                supports_streaming: false,
                duration: Duration::ZERO,
                w: 0,
                h: 0,
                round_message: false,
            });
        }

        msg.reply(input_msg).await?;
        Ok(())
    }

    async fn handle_callback(&self, query: CallbackQuery) -> Result<()> {
        match query.data() {
            CALLBACK_CANCEL => self.handle_cancel(query).await,
            _ => Ok(()),
        }
    }

    async fn handle_cancel(&self, query: CallbackQuery) -> Result<()> {
        let chat_id = query.chat().id();
        let started_by_user_id = match self.started_by.get(&chat_id) {
            Some(id) => *id,
            None => return Ok(()),
        };

        if !self
            .is_authorized_to_cancel(&query, started_by_user_id)
            .await?
        {
            return Ok(());
        }

        self.cancel_upload(&query, chat_id).await
    }

    async fn is_authorized_to_cancel(
        &self,
        query: &CallbackQuery,
        started_by_user_id: i64,
    ) -> Result<bool> {
        if started_by_user_id != query.sender().id() {
            info!(
                "User {} attempted to cancel another user's upload in chat {}",
                query.sender().id(),
                query.chat().id()
            );

            query
                .answer()
                .alert("⚠️ You can't cancel another user's upload")
                .cache_time(Duration::ZERO)
                .send()
                .await?;

            return Ok(false);
        }
        Ok(true)
    }

    async fn cancel_upload(&self, query: &CallbackQuery, chat_id: i64) -> Result<()> {
        if let Some((chat_id, trigger)) = self.triggers.remove(&chat_id) {
            info!("Cancelling upload in chat {}", chat_id);
            drop(trigger);
            self.started_by.remove(&chat_id);

            query
                .load_message()
                .await?
                .edit("⛔ Upload cancelled")
                .await?;

            query.answer().send().await?;
        }
        Ok(())
    }
}

fn create_http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .build()?)
}

fn is_supported_chat(msg: &Message) -> bool {
    matches!(msg.chat(), Chat::User(_) | Chat::Group(_))
}

fn is_private_chat(msg: &Message) -> bool {
    matches!(msg.chat(), Chat::User(_))
}

fn should_ignore_start_in_group(msg: &Message, command: &Command) -> bool {
    matches!(msg.chat(), Chat::Group(_)) && command.name == "start" && command.via.is_none()
}

struct FileInfo {
    name: String,
    size: usize,
    is_video: bool,
}

fn extract_file_info(response: &reqwest::Response) -> Result<FileInfo> {
    let size = response.content_length().unwrap_or_default() as usize;
    let name = extract_filename(response);
    let is_video = is_video_file(response, &name);

    Ok(FileInfo {
        name,
        size,
        is_video,
    })
}

fn extract_filename(response: &reqwest::Response) -> String {
    extract_filename_from_header(response)
        .or_else(|| extract_filename_from_url(response))
        .unwrap_or_else(|| DEFAULT_FILENAME.to_string())
}

fn extract_filename_from_header(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .map(|v| v.trim())
                .find(|v| v.starts_with("filename="))
        })
        .map(|value| value.trim_start_matches("filename="))
        .map(|value| value.trim_matches('"'))
        .and_then(|name| {
            percent_encoding::percent_decode_str(name)
                .decode_utf8()
                .ok()
        })
        .map(|s| s.to_string())
}

fn extract_filename_from_url(response: &reqwest::Response) -> Option<String> {
    response
        .url()
        .path_segments()
        .and_then(|segments| segments.last())
        .and_then(|name| {
            if name.contains('.') {
                Some(name.to_string())
            } else {
                add_extension_from_content_type(response, name)
            }
        })
        .map(|name| {
            percent_encoding::percent_decode_str(&name)
                .decode_utf8()
                .ok()
                .map(|s| s.to_string())
                .unwrap_or(name)
        })
}

fn add_extension_from_content_type(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .and_then(mime_guess::get_mime_extensions_str)
        .and_then(|ext| ext.first())
        .map(|ext| format!("{}.{}", name, ext))
}

fn is_video_file(response: &reqwest::Response, name: &str) -> bool {
    let is_mp4_mime = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.starts_with("video/mp4"))
        .unwrap_or(false);

    is_mp4_mime || name.to_lowercase().ends_with(".mp4")
}

fn validate_file_size(size: usize) -> Result<(), &'static str> {
    if size == 0 {
        return Err("⚠️ File is empty");
    }
    if size > MAX_FILE_SIZE_BYTES {
        return Err("⚠️ File is too large");
    }
    Ok(())
}

fn create_cancellable_stream(
    response: reqwest::Response,
    chat_id: i64,
    triggers: &Arc<DashMap<i64, Trigger>>,
) -> Valved<impl futures::Stream<Item = Result<tokio_util::bytes::Bytes, std::io::Error>>> {
    let (trigger, stream) = Valved::new(
        response
            .bytes_stream()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err)),
    );
    triggers.insert(chat_id, trigger);
    stream
}

fn create_cancel_button() -> Arc<reply_markup::Inline> {
    Arc::new(reply_markup::inline(vec![vec![button::inline(
        "⛔ Cancel",
        "cancel",
    )]]))
}

fn create_progress_stream(
    stream: Valved<impl futures::Stream<Item = Result<tokio_util::bytes::Bytes, std::io::Error>>>,
    total_size: usize,
    filename: String,
    status: Arc<Mutex<Message>>,
    reply_markup: Arc<reply_markup::Inline>,
) -> impl tokio::io::AsyncRead {
    stream.into_async_read().compat().report_progress(
        Duration::from_secs(PROGRESS_UPDATE_INTERVAL_SECS),
        move |progress| {
            let status = status.clone();
            let filename = filename.clone();
            let reply_markup = reply_markup.clone();

            tokio::spawn(async move {
                let progress_percent = (progress as f64 / total_size as f64) * 100.0;
                let progress_text = bytesize::to_string(progress as u64, true);
                let total_text = bytesize::to_string(total_size as u64, true);

                status
                    .lock()
                    .await
                    .edit(
                        InputMessage::html(format!(
                            "⏳ Uploading <code>{}</code> <b>({:.2}%)</b>\n<i>{} / {}</i>",
                            filename, progress_percent, progress_text, total_text
                        ))
                        .reply_markup(reply_markup.as_ref()),
                    )
                    .await
                    .ok();
            });
        },
    )
}
