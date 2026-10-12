import { createReadStream, existsSync, statSync } from "node:fs";
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
  ".js": "text/javascript; charset=utf-8",
  ".mp4": "video/mp4",
  ".jpg": "image/jpeg",
  ".gif": "image/gif",
  ".html": "text/html; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml; charset=utf-8",
};

const server = createServer((request, response) => {
  const url = new URL(request.url ?? "/", `http://${request.headers.host}`);
  const pathname = url.pathname === "/" ? "/index.html" : url.pathname;
  let requestedPath = normalize(join(dist, pathname));

  if (!requestedPath.startsWith(dist) || !existsSync(requestedPath)) {
    response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    response.end("Not found");
    return;
  }

  if (statSync(requestedPath).isDirectory()) {
    requestedPath = join(requestedPath, "index.html");
  }

  if (!existsSync(requestedPath) || !statSync(requestedPath).isFile()) {
    response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    response.end("Not found");
    return;
  }

  const size = statSync(requestedPath).size;
  const range = request.headers.range;
  if (range) {
    const match = /^bytes=(\d*)-(\d*)$/.exec(range);
    let start = match?.[1] ? Number(match[1]) : 0;
    let end = match?.[2] ? Number(match[2]) : size - 1;
    if (match && !match[1] && match[2]) {
      start = Math.max(0, size - Number(match[2]));
      end = size - 1;
    }
    end = Math.min(end, size - 1);
    if (!match || (!match[1] && !match[2]) || start >= size || start > end) {
      response.writeHead(416, { "content-range": `bytes */${size}` });
      response.end();
      return;
    }
    response.writeHead(206, {
      "content-type": contentTypes[extname(requestedPath)] ?? "application/octet-stream",
      "accept-ranges": "bytes",
      "content-range": `bytes ${start}-${end}/${size}`,
      "content-length": end - start + 1,
    });
    if (request.method === "HEAD") response.end();
    else createReadStream(requestedPath, { start, end }).pipe(response);
    return;
  }

  response.writeHead(200, {
    "accept-ranges": "bytes",
    "content-length": size,
    "content-type": contentTypes[extname(requestedPath)] ?? "application/octet-stream",
  });
  if (request.method === "HEAD") { response.end(); return; }
  createReadStream(requestedPath)
    .on("error", () => {
      if (!response.headersSent) {
        response.writeHead(500, { "content-type": "text/plain; charset=utf-8" });
      }
      response.end("Internal server error");
    })
    .pipe(response);
});

server.listen(port, host, () => {
  console.log(`Preview server running at http://${host}:${server.address().port}/`);
});
