import { Bot, InputFile } from "grammy";
import { botToken } from "./config";
import { isValidURL } from "./utils/URLcheck";
import { uploader } from "./utils/uploadFromURL";

const bot = new Bot(botToken as string);

export const getBot = async () => {
  bot.catch(console.error);

  return bot;
};

bot.command("upload", async (ctx) => {
  const url = ctx.match;

  if (!url) {
    ctx.reply("URL не был введен");
  } else if (!isValidURL(url)) {
    ctx.reply(`${url} не ялвялется URL`);
  } else {
    try {
      const fileName = await uploader(url);
      ctx.replyWithDocument(new InputFile(fileName));
    } catch (error) {
      ctx.reply("Произошла ошибка при загрузке файла");
      console.error(error);
    }
  }
});
