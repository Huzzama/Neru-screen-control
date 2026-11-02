import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import "./styles/app.css";

// The webview is not a browser: no context menu, no accidental navigation.
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
  e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
