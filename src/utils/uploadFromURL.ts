import axios from "axios";
import fs from "fs";

export const uploader = async (url: string) => {
  return await axios({
    method: "get",
    url,
    responseType: "stream",
  })
    .then((res) => {
      const disposition = res.headers["content-disposition"];
      const fileName = `.cache/${
        disposition && disposition.includes("filename=")
          ? disposition.split("filename=")[1].replace(/["']/g, "")
          : "downloaded_file"
      }`;
      res.data.pipe(fs.createWriteStream(fileName));
      return fileName;
    })
    .catch(() => {
      const errPng = "src/assets/images/error-404.png";
      return errPng;
    });
};
