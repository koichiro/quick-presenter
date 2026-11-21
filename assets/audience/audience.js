// SPDX-License-Identifier: Apache-2.0
// The duplicate-key token pass follows packages/room-core/src/json.ts (Apache-2.0).
function strictJSON(text) {
  if (new TextEncoder().encode(text).length > 4096) throw new Error("Message too large");
  const value = JSON.parse(text);
  const tokens = text.match(/"(?:[^"\\]|\\.)*"|[{}[\]:,]|[^\s{}[\]:,]+/g) ?? [];
  const stack = [];
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (token === "{" || token === "[") {
      stack.push(token === "{" ? new Set() : null);
      if (stack.length > 32) throw new Error("JSON nesting too deep");
    } else if (token === "}" || token === "]") stack.pop();
    else if (token.startsWith('"') && tokens[i + 1] === ":") {
      const keys = stack.at(-1);
      const key = JSON.parse(token);
      if (keys?.has(key)) throw new Error("Duplicate JSON key");
      keys?.add(key);
    }
  }
  return value;
}

function validServerMessage(message) {
  const object = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
  const exact = (value, names) =>
    object(value) &&
    Object.keys(value).length === names.length &&
    names.every((name) => Object.hasOwn(value, name));
  const id = (value) => typeof value === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(value);
  const capability = (value) => {
    if (typeof value !== "string") return false;
    const chars = [...value];
    return (
      chars.length >= 1 &&
      chars.length <= 64 &&
      chars.every((char) => {
        const point = char.codePointAt(0);
        return point < 0xd800 || point > 0xdfff;
      })
    );
  };
  if (!object(message) || message.protocol !== "qp.audience" || message.version !== "1")
    return false;
  const base = ["protocol", "version", "type"];
  switch (message.type) {
    case "welcome":
      return (
        exact(message, [...base, "connection_id", "capabilities", "available", "limits"]) &&
        id(message.connection_id) &&
        Array.isArray(message.capabilities) &&
        message.capabilities.every(capability) &&
        new Set(message.capabilities).size === message.capabilities.length &&
        typeof message.available === "boolean" &&
        exact(message.limits, ["max_message_bytes", "request_cache_ms"]) &&
        message.limits.max_message_bytes === 4096 &&
        Number.isInteger(message.limits.request_cache_ms) &&
        message.limits.request_cache_ms >= 1 &&
        message.limits.request_cache_ms <= 60000
      );
    case "state":
      return (
        exact(message, [...base, "revision", "available"]) &&
        typeof message.revision === "string" &&
        /^(0|[1-9][0-9]{0,19})$/.test(message.revision) &&
        typeof message.available === "boolean"
      );
    case "result":
      return (
        exact(message, [...base, "request_id", "status"]) &&
        id(message.request_id) &&
        [
          "accepted",
          "busy",
          "stopped",
          "unavailable",
          "unknown",
          "expired",
          "denied",
          "policy_unavailable",
        ].includes(message.status)
      );
    default:
      return false;
  }
}

export function mountAudience({
  location,
  history,
  document,
  WebSocket,
  URL,
  URLSearchParams,
  sessionStorage,
  TextEncoder,
  setTimeout,
  clearTimeout,
}) {
  const status = document.querySelector("#connection-status");
  const feedback = document.querySelector("#feedback");
  const retry = document.querySelector("#retry");
  const form = document.querySelector("form");
  const messageField = document.querySelector("#message");
  const nickname = document.querySelector("#nickname");
  const sendButton = document.querySelector("#send-comment");
  const reactions = [...document.querySelectorAll("[data-kind]")];
  const storageKey = `qp.audience.join:${location.pathname}`;
  let token = new URLSearchParams(location.hash.slice(1)).get("k");
  try {
    if (token !== null) {
      if (/^[A-Za-z0-9_-]{43}$/.test(token)) sessionStorage.setItem(storageKey, token);
      else sessionStorage.removeItem(storageKey);
    } else token = sessionStorage.getItem(storageKey);
    history.replaceState({}, "", location.pathname);
  } catch {
    // Keeping the fragment allows reloads even when tab storage is blocked.
  }
  let socket;
  let authenticated = false;
  let available = false;
  let sequence = 0;
  let pending = null;
  let timeout;
  const updateControls = () => {
    reactions.forEach((button) => {
      button.disabled = !authenticated || !available || pending !== null;
    });
    sendButton.disabled = !authenticated || !available || pending !== null;
  };
  const clearPending = () => {
    clearTimeout(timeout);
    pending = null;
    updateControls();
  };
  const transmit = (payload) => {
    if (!authenticated || !available || pending !== null || socket.readyState !== WebSocket.OPEN)
      return;
    const request_id = String(++sequence);
    pending = { request_id, ...payload };
    updateControls();
    feedback.textContent = "Sending…";
    socket.send(JSON.stringify({ protocol: "qp.audience", version: "1", request_id, ...payload }));
    timeout = setTimeout(() => {
      feedback.textContent = "No response. Reconnect to try again.";
      socket.close();
    }, 4000);
  };
  const connect = () => {
    if (!/^[A-Za-z0-9_-]{43}$/.test(token ?? "")) {
      status.textContent = "Open the complete join link or scan the QR code.";
      return null;
    }
    authenticated = false;
    available = false;
    clearPending();
    retry.hidden = true;
    status.textContent = "Connecting…";
    const endpoint = new URL(`/ws/audience/${location.pathname.split("/").at(-1)}`, location.href);
    endpoint.protocol = location.protocol === "https:" ? "wss:" : "ws:";
    const current = new WebSocket(endpoint.href);
    socket = current;
    current.addEventListener("open", () => {
      if (socket === current)
        current.send(
          JSON.stringify({ protocol: "qp.audience", version: "1", type: "hello", token }),
        );
    });
    current.addEventListener("message", ({ data }) => {
      if (socket !== current) return;
      let response;
      try {
        if (typeof data !== "string") throw new Error("Invalid frame");
        response = strictJSON(data);
        if (!validServerMessage(response)) throw new Error("Invalid message");
      } catch {
        authenticated = false;
        available = false;
        updateControls();
        current.close();
        return;
      }
      if (response.type === "welcome" || response.type === "state") {
        if (response.type === "welcome") authenticated = true;
        available = response.available === true;
        status.textContent =
          authenticated && available ? "Connected to the presentation." : "Presenter unavailable.";
        updateControls();
      } else if (response.type === "result" && pending?.request_id === response.request_id) {
        if (response.status === "accepted") {
          feedback.textContent =
            pending.type === "comment" ? "Comment received." : "Reaction received.";
          if (pending.type === "comment" && messageField.value === pending.body)
            messageField.value = "";
        } else {
          feedback.textContent =
            {
              stopped: "This session has ended.",
              busy: "The presentation is busy. Try again shortly.",
              unknown: "Delivery could not be confirmed. Messages are not resent automatically.",
            }[response.status] ?? "Unable to send.";
        }
        clearPending();
      }
    });
    current.addEventListener("close", () => {
      if (socket !== current) return;
      authenticated = false;
      available = false;
      clearPending();
      status.textContent =
        "Disconnected. Reconnect while the session is active, or scan the new QR code.";
      retry.hidden = false;
    });
    current.addEventListener("error", () => {
      if (socket === current)
        status.textContent = "Unable to connect. Check your network and try again.";
    });
    return current;
  };
  const validateDraft = () => {
    const encoder = new TextEncoder();
    messageField.setCustomValidity(
      messageField.value.trim().length === 0
        ? "Enter a comment."
        : [...messageField.value].length > 200 || encoder.encode(messageField.value).length > 800
          ? "Use up to 200 characters and 800 UTF-8 bytes."
          : "",
    );
    nickname.setCustomValidity(
      [...nickname.value].length > 32 || encoder.encode(nickname.value).length > 128
        ? "Use up to 32 characters and 128 UTF-8 bytes."
        : "",
    );
    return messageField.validity.valid && nickname.validity.valid;
  };
  for (const button of reactions)
    button.addEventListener("click", () =>
      transmit({ type: "reaction", kind: button.dataset.kind }),
    );
  messageField.addEventListener("input", validateDraft);
  nickname.addEventListener("input", validateDraft);
  messageField.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" || event.shiftKey || event.isComposing || event.keyCode === 229)
      return;
    event.preventDefault();
    if (!sendButton.disabled) form.requestSubmit();
  });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (sendButton.disabled) return;
    if (validateDraft())
      transmit({ type: "comment", body: messageField.value, handle: nickname.value });
    else form.reportValidity();
  });
  retry.addEventListener("click", connect);
  return connect();
}
if (typeof window !== "undefined") mountAudience(window);
