import React from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { mangaApi } from "./apiClient";

window.mangaApi = mangaApi;

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
