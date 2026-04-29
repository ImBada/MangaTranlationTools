import type { MangaApi } from "./apiClient";

declare global {
  interface Window {
    mangaApi: MangaApi;
  }
}

export {};
