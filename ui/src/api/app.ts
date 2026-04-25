import { Hono } from "hono";
import { serveStatic } from "@hono/node-server/serve-static";
import { tasks } from "./routes/tasks.js";

/**
 * Create API routes without static file serving.
 * Static serving is configured separately for dev vs prod.
 */
const api = new Hono()
  .get("/health", (c) => {
    return c.json({ status: "ok" });
  })
  .route("/api/tasks", tasks)
  // Catch-all for undefined API routes
  .all("/api/*", (c) => c.json({ error: "Not found" }, 404));

/**
 * Create full app with static file serving.
 *
 * @param staticRoot - Path to static files (relative to cwd).
 *   - Dev: "./dist" (relative to ui/)
 *   - Prod: Uses OVERSEER_UI_STATIC_ROOT env var, fallback "./static"
 */
export function createApp(staticRoot?: string) {
  const root = staticRoot ?? process.env.OVERSEER_UI_STATIC_ROOT ?? "./static";

  return new Hono()
    .route("/", api)
    // Serve static files
    .use("/*", serveStatic({ root }))
    // Fallback to index.html for SPA routing
    .get("/*", serveStatic({ root, path: "index.html" }));
}

export { api };

// Export AppType based on createApp return type
export type AppType = ReturnType<typeof createApp>;
