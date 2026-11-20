import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { App } from "./App";
import { SessionProvider } from "./lib/session";
import { ToastProvider } from "./lib/toast";
import { initI18n, useLang } from "./lib/i18n";
import "./styles.css";

// Apply the saved theme before the first paint.
try {
  const t = localStorage.getItem("xbnc-theme");
  const dark = t ? t === "dark" : window.matchMedia("(prefers-color-scheme: dark)").matches;
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  if (dark) document.querySelector('meta[name="theme-color"]')?.setAttribute("content", "#000000");
} catch {
  /* ignore */
}

// Changing the language re-renders the whole interface with the new texts.
function LangRoot() {
  const lang = useLang();
  return <App key={lang} />;
}

// The texts of the language in use are loaded before the first render.
void initI18n().then(() => createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter>
      <ToastProvider>
        <SessionProvider>
          <LangRoot />
        </SessionProvider>
      </ToastProvider>
    </BrowserRouter>
  </StrictMode>,
));
