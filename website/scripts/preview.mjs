import { createReadStream, existsSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const dist = join(root, "dist");
const host = process.env.HOST ?? "127.0.0.1";
const port = Number(process.env.PORT ?? 4173);

const contentTypes = {
  ".css": "text/css; charset=utf-8",
  ".gif": "image/gif",
  ".html": "text/html; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml; charset=utf-8",
};

const server = createServer((request, response) => {
  const url = new URL(request.url ?? "/", `http://${request.headers.host}`);
  const pathname = url.pathname === "/" ? "/index.html" : url.pathname;
  const requestedPath = normalize(join(dist, pathname));

  if (!requestedPath.startsWith(dist) || !existsSync(requestedPath)) {
    response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    response.end("Not found");
    return;
  }

  response.writeHead(200, {
    "content-type": contentTypes[extname(requestedPath)] ?? "application/octet-stream",
  });
  createReadStream(requestedPath).pipe(response);
});

server.listen(port, host, () => {
  console.log(`Preview server running at http://${host}:${port}/`);
});
