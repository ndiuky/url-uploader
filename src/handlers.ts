import { Context, InputFile, BotError, GrammyError, HttpError } from "grammy";
import { isValidURL } from "./utils/URLcheck";
import { uploader } from "./utils/uploadFromURL";

// TODO: add I18N
export const startHandler = async (ctx: Context) => {
  const name: string = ctx.from?.first_name || "Аноним";

  await ctx.reply(
    `*🖐 Привет, ${name}!* \n\n` +
      "Я бот, который поможет тебе загрузить файл с URL.\n" +
      "Просто отправь мне \`/upload <URL>\` и я загружу файл с него.",
    {
      parse_mode: "Markdown",
    },
  );
};

export const urlUploadHandler = async (ctx: Context) => {
  const url = ctx.match as string;

  if (!url) {
    ctx.reply("URL не был введен");
  } else if (!isValidURL(url)) {
    ctx.reply(`${url} не ялвялется URL`);
  } else {
    try {
      const fileName = await uploader(url);
      ctx.replyWithDocument(new InputFile(fileName));
    } catch (error) {
      ctx.reply("⚠ файл пустой или не существует");
      console.error(error);
    }
  }
};

export const catchHandler = (q: BotError) => {
  const { ctx, error } = q;

  console.error(`Error while handling update ${ctx.update.update_id}:`);

  if (error instanceof GrammyError) {
    console.error("Error in request:", error.description);
  } else if (error instanceof HttpError) {
    console.error("Could not contact Telegram:", error);
  } else {
    console.error("Unknown error:", error);
  }
};
