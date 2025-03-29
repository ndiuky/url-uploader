export const isValidURL = (url: string) => {
  const regex = /^(https?:\/\/)?([\w-]+(\.[\w-]+)+)(\/[\w-./?%&=]*)?$/;
  return regex.test(url);
};
