// The copper headline's sheen, kept in step with the hero fan's light.

// Its own scope, since every script on the page shares one.
(() => {
  const root = document.documentElement;
  const still = matchMedia("(prefers-reduced-motion: reduce)");

  // The sheen sits where the fan's light is: across the headline as the light crosses the fan. It moves a whole
  // repeat of the copper's pattern each sweep, so a sweep ends looking as the next begins. Held still, it rests in the
  // middle, where the stylesheet puts it.
  const hero = marks.find((m) => m.hero);
  function sheen(now) {
    if (still.matches) {
      root.style.removeProperty("--sheen");
    } else {
      const t = (now - start) / 1000;
      root.style.setProperty("--sheen", `${(1 - ((t / hero.sweepS) % 1)) * 100}%`);
    }
    requestAnimationFrame(sheen);
  }
  requestAnimationFrame(sheen);
})();
