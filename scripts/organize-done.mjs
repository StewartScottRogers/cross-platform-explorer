#!/usr/bin/env node
// Auto-archive Tickets/Done/ (CPE-865, epic — /ticketing-organize logic as a committed script).
//
// Subdivides any folder under Done/ that holds >= THRESHOLD .md files into YYYY / QN / MonthName / Week-NN,
// filed by each ticket's `closed:` date. Idempotent + resumable (only moves when over threshold; re-running
// an organized tree is a no-op) and it ONLY moves files — never edits their content. Undated tickets stay
// where they are. Reused by /ticketing-organize (manual), /ticketing-work (on close), and a SessionStart
// hook (universal safety net).
//
// Usage:
//   node scripts/organize-done.mjs            # organize, print a summary, leave changes uncommitted
//   node scripts/organize-done.mjs --commit   # organize; if it moved anything AND we're on `main`, commit
//                                             # it (chore) so a hook never leaves a dirty tree or pollutes
//                                             # a feature branch.
import { readdirSync, readFileSync, mkdirSync, renameSync, existsSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";

const THRESHOLD = 50;
const REPO = join(dirname(fileURLToPath(import.meta.url)), "..");
const DONE = join(REPO, "Tickets", "Done");
const MONTHS = ["January","February","March","April","May","June","July","August","September","October","November","December"];

let moved = 0, created = 0, skipped = 0, warned = 0;

/** ISO-8601 week number (matches GNU `date +%V`), so new tickets file into the same Week-NN folders. */
function isoWeek(y, mo, dd) {
  const date = new Date(Date.UTC(y, mo - 1, dd));
  const day = (date.getUTCDay() + 6) % 7;      // Mon=0..Sun=6
  date.setUTCDate(date.getUTCDate() - day + 3); // nearest Thursday
  const firstThu = new Date(Date.UTC(date.getUTCFullYear(), 0, 4));
  const firstDay = (firstThu.getUTCDay() + 6) % 7;
  firstThu.setUTCDate(firstThu.getUTCDate() - firstDay + 3);
  return 1 + Math.round((date - firstThu) / 604800000);
}

/** Parse a ticket's `closed: YYYY-MM-DD`; null if missing/blank. */
function closedOf(file) {
  const m = readFileSync(file, "utf8").match(/^closed:\s*(\d{4})-(\d{2})-(\d{2})/m);
  return m ? { y: +m[1], mo: +m[2], dd: +m[3] } : null;
}

/** Subfolder name for a file at a directory depth (0 Done root -> year, 1 -> quarter, 2 -> month, 3 -> week). */
function subName(depth, c) {
  switch (depth) {
    case 0: return String(c.y);
    case 1: return "Q" + Math.ceil(c.mo / 3);
    case 2: return MONTHS[c.mo - 1];
    case 3: return "Week-" + String(isoWeek(c.y, c.mo, c.dd)).padStart(2, "0");
    default: return null;
  }
}

function processDir(dir, depth) {
  const entries = readdirSync(dir, { withFileTypes: true });
  const mdFiles = entries.filter((e) => e.isFile() && e.name.endsWith(".md")).map((e) => e.name);
  if (mdFiles.length >= THRESHOLD) {
    if (depth >= 4) {
      console.log(`[WARN] ${dir} has ${mdFiles.length} files at max depth`);
      warned++;
    } else {
      for (const name of mdFiles) {
        const src = join(dir, name);
        const c = closedOf(src);
        if (!c) { console.log(`[SKIP] ${name} — no/blank closed date`); skipped++; continue; }
        const sub = subName(depth, c);
        if (!sub) { console.log(`[SKIP] ${name} — bad date`); skipped++; continue; }
        const destDir = join(dir, sub);
        if (!existsSync(destDir)) { mkdirSync(destDir, { recursive: true }); created++; }
        renameSync(src, join(destDir, name));
        moved++;
      }
    }
  }
  // Always descend into existing subfolders (resumable across a partial prior run).
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.isDirectory()) processDir(join(dir, e.name), depth + 1);
  }
}

if (!existsSync(DONE)) { console.log("no Tickets/Done — nothing to do"); process.exit(0); }
processDir(DONE, 0);
console.log(`RESULT moved=${moved} created=${created} skipped=${skipped} warned=${warned}`);

if (process.argv.includes("--commit") && moved > 0) {
  try {
    const branch = execSync("git rev-parse --abbrev-ref HEAD", { cwd: REPO }).toString().trim();
    if (branch === "main") {
      execSync("git add Tickets/Done", { cwd: REPO });
      // Only commit if staging actually produced a change.
      try { execSync("git diff --cached --quiet", { cwd: REPO }); }
      catch {
        execSync('git commit -m "chore: auto-archive done tickets"', { cwd: REPO });
        console.log("committed auto-archive on main");
      }
    } else {
      console.log(`not committing (on '${branch}', not main) — archive moves left for the next commit`);
    }
  } catch (e) {
    console.log("auto-commit skipped: " + (e.message || e));
  }
}
