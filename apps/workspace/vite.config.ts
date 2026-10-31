import { defineConfig, type Plugin } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
const execute = promisify(execFile);
// Development-only adapter to the same Rust projection used by the desktop.
function localHub(): Plugin {
  let busy = false;
  return {
    name: "workspace-local-hub",
    configureServer(server) {
      server.middlewares.use("/__workspace/hub", async (request, response) => {
        const origin = request.headers.origin;
        const host = request.headers.host;
        if (
          request.method !== "GET" ||
          request.headers["sec-fetch-site"] === "cross-site" ||
          (origin && origin !== `http://${host}`) ||
          (host !== "127.0.0.1:1428" && host !== "localhost:1428")
        ) {
          response.writeHead(403).end();
          return;
        }
        if (busy) {
          response.writeHead(409).end();
          return;
        }
        busy = true;
        try {
          const check =
            new URL(request.url ?? "/", "http://localhost").searchParams.get(
              "check",
            ) === "1";
          const component = new URL(
            request.url ?? "/",
            "http://localhost",
          ).searchParams.get("component");
          if (
            component !== null &&
            !/^[a-z0-9][a-z0-9-]{0,63}$/.test(component)
          ) {
            response.writeHead(400).end();
            return;
          }
          const binary = fileURLToPath(
            new URL(
              `./src-tauri/target/debug/tinnci-workspace${process.platform === "win32" ? ".exe" : ""}`,
              import.meta.url,
            ),
          );
          const { stdout } = await execute(
            binary,
            [
              "--hub-json",
              ...(check ? ["--check"] : []),
              ...(component ? ["--component", component] : []),
            ],
            {
              timeout: 75000,
              maxBuffer: 4 * 1024 * 1024,
            },
          );
          response
            .writeHead(200, {
              "Content-Type": "application/json",
              "Cache-Control": "no-store",
            })
            .end(stdout);
        } catch {
          response.writeHead(503).end('{"error":"hub_unavailable"}');
        } finally {
          busy = false;
        }
      });
    },
  };
}
export default defineConfig({
  plugins: [tailwindcss(), localHub()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1428,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: { target: "es2022" },
});
