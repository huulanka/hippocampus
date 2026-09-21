import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initApiBaseUrl } from "./api";

// Resolved before the first render so no screen's first fetch races the
// settings load and hits the wrong backend for one frame.
initApiBaseUrl().finally(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
