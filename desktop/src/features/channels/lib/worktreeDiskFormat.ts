export function formatDiskBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return "—";
  const gb = bytes / (1024 * 1024 * 1024);
  if (gb >= 1) return `${gb.toFixed(gb >= 10 ? 0 : 1)} GB`;
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${Math.round(mb)} MB`;
  const kb = bytes / 1024;
  return `${Math.max(1, Math.round(kb))} KB`;
}
