/**
 * P5 milestone recording (plan/phases/P05-lateral-interaction.md): the viewer shows two-wheelers
 * filtering and seeping through a stopped queue. Starts a live session of
 * scenarios/barrier_queue.yaml (a barrier closed from 60 s to 180 s), frames the 120 m before the
 * stop line, and records the first closure and the discharge.
 *
 *   marg serve &
 *   node web/e2e/p5_seepage.cjs          # BASE=http://127.0.0.1:8080 OUT=reports/phases/P5
 *
 * Writes seepage.webm (convert with ffmpeg, see reports/phases/P5.md) and screenshots. Needs the
 * `playwright` package and a Chromium it can launch. Not run in CI.
 */
const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

const BASE = process.env.BASE || "http://127.0.0.1:8080";
const OUT = process.env.OUT || "reports/phases/P5";
/** Where the stop line sits across the map (fraction of its width from the left). */
const STOP_LINE_AT = Number(process.env.STOP_LINE_AT || 0.8);
const STOP_LINE_M = 600;
/** Mouse-wheel steps to zoom in (each is deck.gl's 2 / (1 + e^-2) ≈ 1.76× at the viewer's scroll speed). */
const ZOOM_STEPS = Number(process.env.ZOOM_STEPS || 1);
const STEP_FACTOR = 2 / (1 + Math.exp(-2));
/** Share of a mouse drag that moves the map: measured in headless Chromium (SwiftShader) against
 * the queue front at the stop line. Adjust if the framing is off on another browser. */
const PAN_GAIN = Number(process.env.PAN_GAIN || 0.85);
/** Road width (m) and the share of the map's height the viewer fits it to at first (render/MapView.tsx). */
const ROAD_M = 10.5;
const MIN_ROAD_SHARE = 0.1;

const simTime = () => {
  const txt = document.querySelector(".clock")?.textContent ?? "";
  const [h, m, s] = txt.split(" / ")[0].split(":").map(Number);
  return h * 3600 + m * 60 + s;
};

async function playUntil(page, t) {
  await page.fill('input[aria-label="breakpoint time"]', String(t));
  await page.click('button:text("Break at t")');
  await page.click('button:text-is("Play")');
  await page.waitForFunction(
    ([t, f]) =>
      new Function(`return (${f})()`)() >= t &&
      [...document.querySelectorAll("button")].some((b) => b.textContent === "Play"),
    [t, simTime.toString()],
    { timeout: 300000 },
  );
}

(async () => {
  fs.mkdirSync(OUT, { recursive: true });
  const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
  const size = { width: 1280, height: 800 };
  const context = await browser.newContext({ viewport: size, recordVideo: { dir: OUT, size } });
  const page = await context.newPage();
  const shot = (name) => page.screenshot({ path: path.join(OUT, name) });

  await page.goto(`${BASE}/#live`);
  await page.waitForTimeout(1000);
  await page.selectOption('select:near(:text("Scenario"))', "barrier_queue.yaml");
  await page.click("text=Start session");
  await page.waitForSelector('button:text-is("Play")');
  await page.waitForTimeout(1000);

  // Frame the stretch before the stop line. The viewer opens with the road's
  // start at the left edge (−5 m) at MIN_ROAD_SHARE of the map height; zoom in
  // about a point near the left edge, then drag the road to the left.
  const box = await page.locator("canvas").first().boundingBox();
  const cy = box.y + box.height / 2;
  const s0 = (box.height * MIN_ROAD_SHARE) / ROAD_M;
  const anchor = 60;
  for (let i = 0; i < ZOOM_STEPS; i++) {
    await page.mouse.move(box.x + anchor, cy);
    await page.mouse.wheel(0, -100);
    await page.waitForTimeout(150);
  }
  const scale = s0 * STEP_FACTOR ** ZOOM_STEPS;
  const leftNow = -5 + anchor / s0 - anchor / scale;
  const leftWanted = STOP_LINE_M - STOP_LINE_AT * (box.width / scale);
  // The viewer absorbs part of each drag (measured: see PAN_GAIN); correct for it.
  let pan = ((leftWanted - leftNow) * scale) / PAN_GAIN;
  while (pan > 1) {
    const d = Math.min(pan, box.width - 80);
    await page.mouse.move(box.x + box.width - 40, cy);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width - 40 - d, cy, { steps: 8 });
    await page.mouse.up();
    pan -= d;
    await page.waitForTimeout(100);
  }
  console.log(`scale ${scale.toFixed(2)} px/m; view ${leftWanted.toFixed(0)}–${(leftWanted + box.width / scale).toFixed(0)} m`);

  // Arrivals, the closure (60–180 s) and the discharge, at 5× real time.
  await page.selectOption('select:near(:text("Speed"))', "5");
  await playUntil(page, 120);
  await shot("seepage_120s.png");
  await playUntil(page, 178);
  await shot("seepage_178s.png");
  await playUntil(page, 215);
  await shot("seepage_215s.png");

  const video = await page.video().path();
  await context.close();
  await browser.close();
  fs.renameSync(video, path.join(OUT, "seepage.webm"));
  console.log(`wrote ${OUT}/seepage.webm and screenshots`);
})();
