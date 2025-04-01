import axios from "axios";
import fs from "fs";
import path from "path";
import { pipeline } from "stream";
import { promisify } from "util";

const pipe = promisify(pipeline);
const CACHE_DIR = path.resolve(".cache");

export const uploader = async (url: string): Promise<string> => {
  if (!fs.existsSync(CACHE_DIR)) {
    fs.mkdirSync(CACHE_DIR, { recursive: true });
  }

  const response = await axios.get(url, {
    responseType: "stream",
    headers: {
      "User-Agent":
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.3",
    },
  });
  if (response.status !== 200) {
    throw new Error("Invalid response code: " + response.status);
  } else if (!response.data) {
    throw new Error("Empty response");
  } else {
    const disposition = response.headers["content-disposition"];
    const fileName = path.basename(
      disposition.includes("filename=")
        ? disposition.split("filename=")[1].replace(/["']/g, "")
        : "unknown",
    );
    const filePath = path.join(CACHE_DIR, fileName);

    await pipe(response.data, fs.createWriteStream(filePath));
    return filePath;
  }
};
