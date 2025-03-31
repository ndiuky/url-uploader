import { Context, InputFile } from "grammy";
import { isValidURL } from "./utils/URLcheck";
import { uploader } from "./utils/uploadFromURL";

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
      ctx.reply("Произошла ошибка при загрузке файла");
      console.error(error);
    }
  }
};
