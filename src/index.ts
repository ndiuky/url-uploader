import { getBot } from "./bot";

const bot = getBot();

void (async () => {
  (await bot).start();
  console.log("bot started");
})();
