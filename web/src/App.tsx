import { describeScenario } from "./scenario";

/** Viewer shell. The live, replay and compare views arrive in phase P4. */
export function App() {
  return (
    <main style={{ fontFamily: "system-ui, sans-serif", padding: 24 }}>
      <h1>marg viewer</h1>
      <p>Live, replay and compare views are built in phase P4 (see plan/phases/P04-web-viewer.md).</p>
      <p>{describeScenario({ name: "toy", duration_s: 600, toy_arrivals_per_h: 1800 })}</p>
    </main>
  );
}
