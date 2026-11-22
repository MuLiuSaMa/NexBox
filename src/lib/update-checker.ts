"use client";

const GITCODE_OWNER = "MuLiuSaMa";
const GITCODE_REPO = "nexbox";
const GITCODE_WEB = "https://gitcode.com";

// GitCode 全局只读令牌（Repository「bash 客户端的上传下载」仅只读）。
// 用于 release 接口请求走认证以规避匿名限流；该令牌无写权限，公开仓库下不存在额外风险。
const GITCODE_READ_TOKEN = "WmAt3-nHJiYGKrCby8bGajtw";

export interface ReleaseInfo {
  tag_name: string;
  name: string;
  body: string;
  assets: Array<{
    name: string;
    browser_download_url: string;
    size: number;
  }>;
  published_at: string;
  html_url: string;
}

// GitCode 公开仓库的 release 接口可匿名读取，无需 access_token
const releaseBaseUrl = (path: string, query = "") =>
  `https://api.gitcode.com/api/v5/repos/${GITCODE_OWNER}/${GITCODE_REPO}${path}${query}`;

// GitCode 的 release 响应不含顶层 html_url，按仓库与 tag 拼接
function releaseHtmlUrl(tagName: string): string {
  return `${GITCODE_WEB}/${GITCODE_OWNER}/${GITCODE_REPO}/releases/tag/${tagName}`;
}

// 携带只读令牌的请求头：让 release 接口走认证以提升限流配额
const authenticatedHeaders = () => ({
  "Content-Type": "application/json",
  Authorization: `Bearer ${GITCODE_READ_TOKEN}`,
});

export async function fetchLatestRelease(): Promise<ReleaseInfo | null> {
  try {
    const response = await fetch(releaseBaseUrl("/releases/latest"), {
      headers: authenticatedHeaders(),
    });

    if (!response.ok) {
      return null;
    }

    const data = await response.json();
    return { ...data, html_url: releaseHtmlUrl(data.tag_name) };
  } catch {
    return null;
  }
}

export async function fetchAllReleases(): Promise<ReleaseInfo[]> {
  try {
    const response = await fetch(releaseBaseUrl("/releases", "?per_page=100"), {
      headers: authenticatedHeaders(),
    });

    if (!response.ok) {
      return [];
    }

    const data = await response.json();
    return (data as ReleaseInfo[]).map((r) => ({
      ...r,
      html_url: releaseHtmlUrl(r.tag_name),
    }));
  } catch {
    return [];
  }
}

export async function fetchReleaseByTag(tag: string): Promise<ReleaseInfo | null> {
  try {
    const response = await fetch(
      releaseBaseUrl(`/releases/tags/${encodeURIComponent(tag)}`),
      {
        headers: authenticatedHeaders(),
      }
    );

    if (!response.ok) {
      return null;
    }

    const data = await response.json();
    return { ...data, html_url: releaseHtmlUrl(data.tag_name) };
  } catch {
    return null;
  }
}

export function compareVersions(current: string, latest: string): boolean {
  const cleanCurrent = current.replace(/^v/, "");
  const cleanLatest = latest.replace(/^v/, "");

  const currentParts = cleanCurrent.split(".").map(Number);
  const latestParts = cleanLatest.split(".").map(Number);

  for (let i = 0; i < Math.max(currentParts.length, latestParts.length); i++) {
    const currentPart = currentParts[i] || 0;
    const latestPart = latestParts[i] || 0;

    if (latestPart > currentPart) return true;
    if (latestPart < currentPart) return false;
  }

  return false;
}