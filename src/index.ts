import { Bot } from "grammy";
import { botToken } from "./config";
import { urlUploadHandler } from "./handlers";

const bot = new Bot(botToken as string);

bot.command("upload", urlUploadHandler);

void (async () => {
  await bot.init();
  console.info("Bot started! @" + bot.botInfo.username);
  await bot.start();
})();
