/**
 * Format an epoch-millisecond timestamp the way Explorer's "Date modified"
 * column does: M/D/YYYY h:mm AM/PM. Returns "" when there is no timestamp.
 */
export function formatDate(epochMs: number | null): string {
  if (epochMs === null || Number.isNaN(epochMs)) return "";

  const d = new Date(epochMs);
  if (Number.isNaN(d.getTime())) return "";

  const month = d.getMonth() + 1;
  const day = d.getDate();
  const year = d.getFullYear();

  let hours = d.getHours();
  const meridiem = hours >= 12 ? "PM" : "AM";
  hours = hours % 12;
  if (hours === 0) hours = 12; // midnight and noon are both "12"

  const minutes = String(d.getMinutes()).padStart(2, "0");

  return `${month}/${day}/${year} ${hours}:${minutes} ${meridiem}`;
}
