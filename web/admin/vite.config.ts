import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

// In development the dashboard runs on Vite's server and forwards API calls to a local Clavren
// server, so cookies stay same-origin as they are in production (where the binary serves both).
const CLAVREN_DEV_SERVER = "http://127.0.0.1:8080";

/** In development, `/apply` and the pages under it get the application form, as the server does. */
function applyPages(): Plugin {
  return {
    name: "apply-pages",
    configureServer(server) {
      server.middlewares.use((request, _response, next) => {
        const path = request.url?.split(/[?#]/)[0] ?? "";
        if (path === "/apply" || path.startsWith("/apply/")) request.url = "/apply.html";
        next();
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), applyPages()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // the dashboard, and the public licence application form
    rollupOptions: { input: { index: "index.html", apply: "apply.html" } },
  },
  server: {
    proxy: {
      "/api": CLAVREN_DEV_SERVER,
      "/health": CLAVREN_DEV_SERVER,
    },
  },
});
