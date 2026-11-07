// Fixture shape, calibrated to aggregate measurements on 2026-11-06. See docs/usage.md.
export const SEED = 20261106;
// s: typical. m: longest local session (5,974). l: longest cloud session (14,205).
// xl: headroom only, about 3.5x the real maximum. Bars are decided through l.
export const TIERS = { s: 500, m: 6000, l: 15000, xl: 50000 } as const;
export const MIX = {
  // Share of renderable blocks in the longest cloud session. The remainder is assistant text (17%).
  user: 0.006, tool_use: 0.336, tool_result: 0.336, thinking: 0.151, image: 0.001,
  // Measured p50 chars: text 575, thinking 1,918, tool_result 488.
  text_median: 575, text_max: 60000,
  tool_result_median: 488, tool_result_max: 390000,
  thinking_median: 1918,
  // Bash is 83% of tool uses, MCP tools 12%.
  tool_names: ["Bash", "Bash", "Bash", "Bash", "Bash", "Bash", "Bash", "Bash", "Bash", "Bash", "mcp__github__get_pr", "Read", "Edit"],
};
// Largest single event measured: 1.2 MB in cloud, 2.3 MB locally.
export const GIANT = { one_per: 5000, min_chars: 300000, max_chars: 2400000 };
export const DIFF = { lines: 20000, files: 120 };
