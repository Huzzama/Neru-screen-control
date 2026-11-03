// "Check for updates": the newest GitHub release vs. the running version.
// Done from the webview (api.github.com allows cross-origin reads; the CSP in
// tauri.conf.json lists it), so the Rust side needs no HTTP client.

export const REPO = "Huzzama/Neru-screen-control";
export const RELEASES_URL = `https://github.com/${REPO}/releases`;

export interface UpdateCheck {
  current: string;
  latest: string;
  /** Newer than `current`. */
  available: boolean;
  url: string;
  published: string | null;
  /** The AppImage asset of the latest release, when it has one. */
  appimage: string | null;
}

/** Numeric, dotted comparison: 1 if a > b, -1 if a < b, 0 if equal. A pre-release
 *  suffix ("2.1.0-beta.1") sorts below the plain version. */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) => {
    const [core, pre] = v.trim().replace(/^v/i, "").split("-", 2);
    const nums = core.split(".").map((x) => parseInt(x, 10) || 0);
    while (nums.length < 3) nums.push(0);
    return { nums, pre: pre ?? null };
  };
  const pa = parse(a);
  const pb = parse(b);
  for (let i = 0; i < Math.max(pa.nums.length, pb.nums.length); i++) {
    const d = (pa.nums[i] ?? 0) - (pb.nums[i] ?? 0);
    if (d !== 0) return d > 0 ? 1 : -1;
  }
  if (pa.pre === pb.pre) return 0;
  if (pa.pre === null) return 1;
  if (pb.pre === null) return -1;
  return pa.pre > pb.pre ? 1 : -1;
}

export async function checkForUpdate(current: string, signal?: AbortSignal): Promise<UpdateCheck> {
  const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
    headers: { Accept: "application/vnd.github+json" },
    signal,
  });
  if (res.status === 404) {
    // No release published yet.
    return { current, latest: current, available: false, url: RELEASES_URL, published: null, appimage: null };
  }
  if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
  const rel = (await res.json()) as { tag_name?: string; html_url?: string; published_at?: string; assets?: { name: string; browser_download_url: string }[] };
  const latest = (rel.tag_name ?? "").replace(/^v/i, "");
  if (!latest) throw new Error("release without a tag");
  const appimage = rel.assets?.find((a) => a.name.toLowerCase().endsWith(".appimage"))?.browser_download_url ?? null;
  return {
    current,
    latest,
    available: compareVersions(latest, current) > 0,
    url: rel.html_url ?? RELEASES_URL,
    published: rel.published_at ?? null,
    appimage,
  };
}
