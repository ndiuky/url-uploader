import { Bot } from "grammy";
import { botToken } from "./config";
import { isValidURL } from "./utils/URLcheck";

const bot = new Bot(botToken as string);

export const getBot = async () => {
  bot.catch(console.error);

  return bot;
};

bot.command("upload", (ctx) => {
  const url = ctx.match;

  if (!url) {
    ctx.reply("URL не был введен");
  } else if (!isValidURL(url)) {
    ctx.reply(`${url} не ялвялется URL`);
  } else {
    ctx.reply("это URL");
  }
});
