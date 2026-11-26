// Renders a card of tools/video/cards.html frame by frame into a video
// (Playwright's Chromium; every CSS animation is set to each frame's time).
//   node cards.js <title|end> <width> <height> <seconds> <out.mp4> [fps] [lang]
const { chromium } = require('playwright');
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

(async () => {
  const [card, w, h, seconds, out, fpsArg, lang] = process.argv.slice(2);
  const fps = Number(fpsArg || 60);
  const width = Number(w);
  const height = Number(h);
  const frames = Math.round(Number(seconds) * fps);
  const ffmpeg = spawn('ffmpeg', [
    '-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(fps), '-c:v', 'png',
    '-i', '-', '-c:v', 'libx264', '-preset', 'veryfast', '-crf', '10', '-pix_fmt', 'yuv444p', out,
  ], { stdio: ['pipe', 'inherit', 'inherit'] });
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width, height } });
  const url = 'file://' + path.join(__dirname, 'cards.html') + `?card=${card}&w=${width}&h=${height}&lang=${lang || 'en'}`;
  await page.goto(url);
  // (A mask from a file:// address is refused: the wordmark goes in as data.)
  const wordmark = path.join(__dirname, '../../crates/client/assets/wordmark.png');
  const data = 'data:image/png;base64,' + fs.readFileSync(wordmark).toString('base64');
  await page.evaluate((d) => document.documentElement.style.setProperty('--mask', `url(${d})`), data);
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all([...document.images].map((i) => i.decode().catch(() => {})));
  });
  await page.waitForTimeout(300);
  await page.evaluate(() => document.getAnimations().forEach((a) => a.pause()));
  for (let i = 0; i < frames; i++) {
    const t = (i * 1000) / fps;
    await page.evaluate((t) => document.getAnimations().forEach((a) => { a.currentTime = t; }), t);
    const png = await page.screenshot({ type: 'png' });
    if (!ffmpeg.stdin.write(png)) await new Promise((r) => ffmpeg.stdin.once('drain', r));
  }
  ffmpeg.stdin.end();
  await new Promise((r) => ffmpeg.on('close', r));
  await browser.close();
})();
