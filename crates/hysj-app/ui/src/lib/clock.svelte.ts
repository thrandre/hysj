// A shared, ticking "now" (unix seconds) for countdowns.
export const clock = $state({ now: Date.now() / 1000 });
setInterval(() => (clock.now = Date.now() / 1000), 1000);
