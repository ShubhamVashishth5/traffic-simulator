/**
 * P4 milestone recording (plan/phases/P04-web-viewer.md): change the car desired-speed mean live
 * and watch the fundamental diagram respond; then open an experiment run in replay and filter
 * by class. Drives a running `marg serve` with Playwright and records a video.
 *
 *   marg serve --runs <runs dir with an experiment, e.g. from scripts/p4_twins.py> &
 *   node web/e2e/milestone.cjs            # BASE=http://127.0.0.1:8080 OUT=reports/phases/P4
 *
 * Needs the `playwright` package and a Chromium it can launch. Not run in CI.
 */
const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

const BASE = process.env.BASE || "http://127.0.0.1:8080";
const OUT = process.env.OUT || "reports/phases/P4";
const REPLAY_RUN = process.env.RUN || "";

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
    ([t, f]) => new Function(`return (${f})()`)() >= t && [...document.querySelectorAll("button")].some((b) => b.textContent === "Play"),
    [t, simTime.toString()],
    { timeout: 180000 },
  );
}

(async () => {
  fs.mkdirSync(OUT, { recursive: true });
  const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
  const size = { width: 1280, height: 800 };
  const context = await browser.newContext({ viewport: size, recordVideo: { dir: OUT, size } });
  const page = await context.newPage();
  const shot = (name) => page.screenshot({ path: path.join(OUT, name) });

  // 1. Live: run 15 simulated minutes, then raise every car type's desired-speed mean by 30 %.
  await page.goto(`${BASE}/#live`);
  await page.click("text=Start session");
  await page.waitForSelector('button:text-is("Play")');
  await page.selectOption('select:near(:text("Speed"))', "10");
  await page.click('button:text-is("Play")');
  await page.waitForTimeout(6000);
  await page.click('button:text-is("Pause")');
  await page.selectOption('select:near(:text("Speed"))', "0");
  await playUntil(page, 900);
  await page.locator("text=Fundamental diagram").scrollIntoViewIfNeeded();
  await page.waitForTimeout(2500);
  await shot("live_fd_before.png");
  await page.evaluate(() => window.scrollTo(0, 0));
  for (const t of ["hatchback", "sedan", "suv"]) {
    await page.fill('input[placeholder^="filter"]', `free_flow_speed.${t}`);
    const head = page.locator(".param-head").first();
    if ((await head.getAttribute("aria-expanded")) !== "true") await head.click();
    const mean = page.locator('.editor .fields label:has-text("mean") input');
    const v = Number(await mean.inputValue());
    await mean.fill(String(Math.round(v * 1.3 * 100) / 100));
    await page.waitForTimeout(700);
    await page.click('button:text("Apply and resample existing")');
    await page.waitForTimeout(700);
  }
  await page.click('button:has-text("Changes (")');
  await page.waitForTimeout(1500);
  await page.click('button:has-text("Command log")');
  await page.waitForTimeout(1500);
  await playUntil(page, 1800);
  await page.locator("text=Fundamental diagram").scrollIntoViewIfNeeded();
  await page.waitForTimeout(3000);
  await shot("live_fd_after.png");
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.click('button:text("Save session")');
  await page.waitForSelector("text=Headless replay", { timeout: 120000 });
  await page.waitForTimeout(2500);
  await shot("live_saved.png");

  // 2. Replay an experiment run and filter by class.
  let run = REPLAY_RUN;
  if (!run) {
    const list = await (await page.request.get(`${BASE}/runs`)).json();
    const r = list.runs.find((x) => x.experiment && x.tables.includes("trajectories")) ?? list.runs.find((x) => x.tables.includes("trajectories"));
    run = r.path;
  }
  await page.goto(`${BASE}/#replay?run=${encodeURIComponent(run)}`);
  await page.waitForSelector(".scrubber", { timeout: 60000 });
  const slider = page.locator(".scrubber input[type=range]");
  await slider.evaluate((el) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    set.call(el, "900");
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await page.selectOption(".scrubber select", "10");
  await page.click(".scrubber button.primary");
  await page.waitForTimeout(6000);
  await page.fill('input[placeholder^="e.g. class"]', "class = 'two_wheeler'");
  await page.waitForTimeout(800);
  await page.click('button:text-is("Apply")');
  await page.waitForTimeout(7000);
  await shot("replay_filtered.png");
  await page.click(".scrubber button.primary");
  await page.locator("text=Mean speed, 30 s").scrollIntoViewIfNeeded();
  await page.waitForTimeout(4000);
  await context.close();
  const video = await page.video().path();
  fs.renameSync(video, path.join(OUT, "milestone.webm"));
  await browser.close();
  console.log(`wrote ${OUT}/milestone.webm and screenshots`);
})();
