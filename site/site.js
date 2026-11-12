// The nav gains its edge once the page scrolls, and things come into view once as they're reached.

const nav = document.querySelector(".nav");
const onScroll = () => nav.classList.toggle("scrolled", scrollY > 8);
addEventListener("scroll", onScroll, { passive: true });
onScroll();

const reveal = new IntersectionObserver(
  (entries) => {
    // What arrives together follows in order, a beat apart.
    entries
      .filter((entry) => entry.isIntersecting)
      .forEach((entry, i) => {
        entry.target.style.setProperty("--delay", `${i * 70}ms`);
        entry.target.classList.add("in");
        reveal.unobserve(entry.target);
      });
  },
  { rootMargin: "0px 0px -8% 0px" },
);

document.querySelectorAll(".reveal").forEach((el) => reveal.observe(el));

// The page's first script offers someone on Windows the x64 download, which every Windows runs. Only some browsers
// say what the processor is, and only when asked, so Windows on ARM gets its own download once they answer.

const root = document.documentElement;
if (root.dataset.os === "win") {
  if (/\b(arm64|aarch64)\b/i.test(navigator.userAgent)) root.dataset.os = "win-arm64";
  navigator.userAgentData
    ?.getHighEntropyValues?.(["architecture"])
    .then(({ architecture }) => {
      if (architecture === "arm") root.dataset.os = "win-arm64";
    })
    .catch(() => {});
}

// Words that change with the hero fan's light: a slot changes on the sweep event it names, and an item can hold for
// a few more sweeps with data-rest. Reduce Motion shows the slot's data-still words instead.

const heroFan = document.querySelector("canvas.fan[data-hero]");
const stillMotion = matchMedia("(prefers-reduced-motion: reduce)");

const slots = [...document.querySelectorAll(".rotate")].map((el) => {
  const items = [...el.children];
  const still = document.createElement("span");
  still.className = "still-words";
  still.textContent = el.dataset.still;
  el.append(still);
  return { el, items, still, at: 0, rest: 0, since: 0 };
});

function fit(slot) {
  if (stillMotion.matches) return;
  slot.el.style.width = `${slot.items[slot.at].getBoundingClientRect().width}px`;
}

function reset(slot) {
  slot.at = 0;
  slot.rest = Number(slot.items[0].dataset.rest || 0);
  slot.since = performance.now();
  slot.el.classList.toggle("still", stillMotion.matches);
  for (const item of slot.items) item.classList.remove("on", "off");
  slot.items[0].classList.add("on");
  fit(slot);
}

function advance(slot) {
  // The first words stay for at least a whole sweep, however far through one the page started.
  if (performance.now() - slot.since < 3000) return;
  if (slot.rest > 0) {
    slot.rest -= 1;
    return;
  }
  const leaving = slot.items[slot.at];
  slot.at = (slot.at + 1) % slot.items.length;
  const coming = slot.items[slot.at];
  slot.rest = Number(coming.dataset.rest || 0);
  leaving.classList.replace("on", "off");
  coming.classList.remove("off");
  coming.classList.add("on");
  fit(slot);
  setTimeout(() => leaving.classList.remove("off"), 800);
}

for (const event of ["sweep-start", "sweep-middle"]) {
  heroFan?.addEventListener(event, () => {
    for (const slot of slots) {
      if (slot.el.dataset.on === event) advance(slot);
    }
  });
}

const shown = () => slots.filter((slot) => slot.el.offsetParent);
slots.forEach(reset);
addEventListener("resize", () => shown().forEach(fit));
stillMotion.addEventListener("change", () => slots.forEach(reset));
document.fonts?.ready.then(() => shown().forEach(fit));
