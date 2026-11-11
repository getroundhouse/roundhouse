// The background: halftone glows that radiate from the page's fans, the hero's, the hub's in the diagram and the
// closing mark's, so the sections of text between them stay clean. They're drawn once, and again on resize.

// Its own scope, since every script on the page shares one.
(() => {
  const COPPER = [208, 138, 92];
  const rgba = ([r, g, b], a) => `rgba(${r},${g},${b},${a})`;
  const canvases = [];

  // Where an element is on the page, leaving out transforms such as the ones things come into view with.
  function onPage(el) {
    let x = 0;
    let y = 0;
    for (let at = el; at; at = at.offsetParent) {
      x += at.offsetLeft;
      y += at.offsetTop;
    }
    return { x, y };
  }

  function markOf(canvas) {
    return marks.find((m) => m.canvas === canvas);
  }

  // Each place a texture radiates from: its fan's turntable, where it starts and how far it reaches, and whether it
  // goes all the way round, as the hub's does, or stays a half circle over its baseline like the fan.
  function sources(width) {
    const hero = markOf(document.querySelector("canvas.fan[data-hero]"));
    const heroAt = onPage(hero.canvas);
    const outerCount = hero.fan.first + 4 * (hero.fan.rows - 1);
    const grain = { spacing: hero.fan.spacing, dot: hero.fan.dot };
    const heroX = heroAt.x + hero.fan.cx;
    // The hero's glow reaches to the middle of the app's picture, or down the hero, if there's no picture.
    const shot = document.querySelector(".window") || document.querySelector(".hero");
    const list = [
      {
        x: heroX,
        y: heroAt.y + hero.fan.cy,
        from: (hero.fan.spacing * (outerCount - 1)) / Math.PI,
        reach: Math.max(heroX, width - heroX) * 1.05,
        full: false,
        strength: 1,
        top: 0,
        bottom: onPage(shot).y + shot.offsetHeight / 2,
      },
    ];
    const centre = (selector) => {
      const canvas = document.querySelector(selector);
      if (!canvas) return null;
      const mark = markOf(canvas);
      const at = onPage(mark.canvas);
      return { x: at.x + mark.fan.cx, y: at.y + mark.fan.cy };
    };
    // A page can leave out the hub or the closing mark, and each then glows on its own.
    const hub = centre(".hub-node canvas.fan");
    const home = centre(".home-mark");
    // The hub's circle and the closing mark's dome each fade out halfway to the other, so they never overlap.
    const between = hub && home ? (home.y - hub.y) / 2 : width;
    for (const [at, from, reach, full, strength] of [
      [hub, 150, Math.min(width * 0.5, between + 60), true, 0.7],
      [home, 76, Math.min(width * 0.46, between - 20), false, 0.8],
    ]) {
      if (!at) continue;
      list.push({ ...at, from, reach, full, strength, top: at.y - reach, bottom: full ? at.y + reach : at.y + 2 });
    }
    return list.map((s) => ({ ...grain, ...s }));
  }

  // The glow as halftone: dots on a grid, biggest just outside the fan, shrinking to nothing.
  function halftone(ctx, s, region) {
    const step = 9;
    ctx.fillStyle = rgba(COPPER, 0.32 * s.strength);
    const reach = s.reach * 0.8;
    for (let y = region.top + (step / 2 - (region.top % step)); y < region.bottom; y += step) {
      for (let x = (region.width / 2) % step; x < region.width; x += step) {
        const d = Math.hypot(x - s.x, y - s.y);
        if (d < s.from + s.spacing) continue;
        const t = 1 - (d - s.from) / (reach - s.from);
        if (t <= 0) continue;
        const under = !s.full && y > s.y ? 0.45 : 1;
        const r = 1.9 * t * t * under;
        if (r < 0.25) continue;
        ctx.beginPath();
        ctx.arc(x, y - region.top, r, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  function draw() {
    canvases.splice(0).forEach((c) => c.remove());
    const width = document.documentElement.clientWidth;
    const ratio = devicePixelRatio || 1;
    for (const s of sources(width)) {
      const top = Math.max(0, Math.floor(s.top));
      const bottom = Math.ceil(s.bottom);
      const canvas = document.createElement("canvas");
      canvas.className = "backdrop";
      canvas.setAttribute("aria-hidden", "true");
      canvas.style.top = `${top}px`;
      canvas.style.width = `${width}px`;
      canvas.style.height = `${bottom - top}px`;
      canvas.width = Math.round(width * ratio);
      canvas.height = Math.round((bottom - top) * ratio);
      const ctx = canvas.getContext("2d");
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
      halftone(ctx, s, { top, bottom, width });
      document.body.prepend(canvas);
      canvases.push(canvas);
    }
  }

  draw();

  let redrawing;
  addEventListener("resize", () => {
    cancelAnimationFrame(redrawing);
    redrawing = requestAnimationFrame(draw);
  });
  document.fonts?.ready.then(draw);
})();
