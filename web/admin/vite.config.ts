import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// In development the dashboard runs on Vite's server and forwards API calls to a local Clavren
// server, so cookies stay same-origin as they are in production (where the binary serves both).
const CLAVREN_DEV_SERVER = "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [react()],
  build: { outDir: "dist", emptyOutDir: true },
  server: {
    proxy: {
      "/api": CLAVREN_DEV_SERVER,
      "/health": CLAVREN_DEV_SERVER,
    },
  },
});
