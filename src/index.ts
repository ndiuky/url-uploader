import { Bot } from "grammy";
import { botToken } from "./config";
import { startHandler, urlUploadHandler, catchHandler } from "./handlers";

const bot = new Bot(botToken as string);

bot.command(["start", "help"], startHandler);
bot.command("upload", urlUploadHandler);

bot.catch(catchHandler);

void (async () => {
  await bot.init();
  console.info("Bot started! @" + bot.botInfo.username);
  await bot.start();
})();
