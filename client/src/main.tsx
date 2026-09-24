import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initApiBaseUrl } from "./api";

// The app is laid out for the screen it is on; pinching it bigger or
// smaller only ever took the tab bar and the text out of proportion. The
// viewport tag says so, and WebKit's own pinch events are refused as well
// for the webviews that read that tag as a suggestion. The graph zooms
// itself, from pointer events, and is unaffected.
for (const type of ["gesturestart", "gesturechange", "gestureend"]) {
  document.addEventListener(type, (event) => event.preventDefault(), { passive: false });
}

// Resolved before the first render so no screen's first fetch races the
// settings load and hits the wrong backend for one frame.
initApiBaseUrl().finally(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
