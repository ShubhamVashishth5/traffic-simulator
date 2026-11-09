/** Viewer shell (plan P4): live, replay and compare views over one map component and one style. */
import { useEffect, useState } from "react";
import { CompareView } from "./compare/CompareView";
import { LiveView } from "./live/LiveView";
import { ReplayView } from "./replay/ReplayView";
import { type Chrome, applyChrome, chrome } from "./theme";

type ViewKey = "live" | "replay" | "compare";

function fromHash(): ViewKey {
  const h = window.location.hash.replace(/^#/, "").split("?")[0];
  return h === "replay" || h === "compare" ? h : "live";
}

export function App() {
  const [view, setView] = useState<ViewKey>(fromHash);
  const [theme, setTheme] = useState<Chrome>(chrome);
  useEffect(() => {
    applyChrome(theme);
  }, [theme]);
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setTheme(chrome());
    mq.addEventListener("change", onChange);
    const onHash = () => setView(fromHash());
    window.addEventListener("hashchange", onHash);
    return () => {
      mq.removeEventListener("change", onChange);
      window.removeEventListener("hashchange", onHash);
    };
  }, []);
  return (
    <div className="app">
      <header>
        <h1>marg</h1>
        <nav className="tabs main-tabs">
          {(["live", "replay", "compare"] as const).map((v) => (
            <a key={v} href={`#${v}`} className={view === v ? "active" : ""}>
              {{ live: "Live", replay: "Replay", compare: "Compare twins" }[v]}
            </a>
          ))}
        </nav>
        <span className="muted small">lane-free mixed-traffic simulator · viewer protocol 1</span>
      </header>
      <main>
        {view === "live" && <LiveView theme={theme} />}
        {view === "replay" && <ReplayView theme={theme} />}
        {view === "compare" && <CompareView theme={theme} />}
      </main>
    </div>
  );
}
