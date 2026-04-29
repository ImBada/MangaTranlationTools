import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react-swc";

export default defineConfig({
  root: "src/client",
  base: "./",
  plugins: [react(), tailwindcss()],
  build: {
    outDir: "../../out/client",
    emptyOutDir: false
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    proxy: {
      "/api": "http://127.0.0.1:3000"
    }
  }
});
