const COMMAND_PREFIX: char = '/';
const BOT_MENTION_SEPARATOR: char = '@';
const COMMAND_ARG_SEPARATOR: char = ' ';

#[derive(Debug)]
pub struct Command {
    pub name: String,
    pub arg: Option<String>,
    pub(crate) via: Option<String>,
}

pub fn parse_command(text: &str) -> Option<Command> {
    if !text.starts_with(COMMAND_PREFIX) {
        return None;
    }

    let text_without_prefix = &text[1..];
    let (command_part, arg) = split_command_and_arg(text_without_prefix);
    let (name, via) = split_command_and_bot_mention(command_part)?;

    Some(Command {
        name: name.to_string(),
        via: via.map(|s| s.to_string()),
        arg: arg.map(|s| s.to_string()),
    })
}

fn split_command_and_arg(text: &str) -> (&str, Option<&str>) {
    let mut parts = text.splitn(2, COMMAND_ARG_SEPARATOR);
    let command = parts.next().unwrap_or_default();
    let arg = parts.next();
    (command, arg)
}

fn split_command_and_bot_mention(text: &str) -> Option<(&str, Option<&str>)> {
    let mut parts = text.splitn(2, BOT_MENTION_SEPARATOR);
    let name = parts.next()?;
    let via = parts.next();
    Some((name, via))
}
