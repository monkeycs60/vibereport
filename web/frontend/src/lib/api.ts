const API_URL = import.meta.env.PUBLIC_API_URL || 'https://vibereport-api.clement-serizay.workers.dev';

export interface ReportData {
  id: string;
  repo_name: string;
  ai_ratio: number;
  grade: string;
  score: number;
  roast: string;
  languages: Record<string, number>;
  total_commits: number;
  ai_commits: number;
  total_lines: number;
  created_at: string;
}

export interface StatsData {
  total_reports: number;
  average_ai_percent: number;
  total_lines_analyzed: number;
}

export interface LeaderboardEntry {
  id: string;
  repo_name: string;
  ai_ratio: number;
  grade: string;
  score: number;
  roast: string;
  created_at: string;
}

export interface LeaderboardResponse {
  entries: LeaderboardEntry[];
  total: number;
  page: number;
  limit: number;
}

export async function fetchStats(): Promise<StatsData> {
  try {
    const res = await fetch(`${API_URL}/api/stats`);
    if (!res.ok) throw new Error('Failed to fetch stats');
    const data = await res.json();
    // API returns avg_ai_ratio (0-1), frontend expects average_ai_percent (0-100)
    return {
      total_reports: data.total_reports || 0,
      average_ai_percent: Math.round((data.avg_ai_ratio || 0) * 100),
      total_lines_analyzed: data.total_lines_analyzed || 0,
    };
  } catch {
    return { total_reports: 0, average_ai_percent: 0, total_lines_analyzed: 0 };
  }
}

export async function fetchReport(id: string): Promise<ReportData | null> {
  try {
    const res = await fetch(`${API_URL}/api/reports/${id}`);
    if (!res.ok) return null;
    return res.json();
  } catch {
    return null;
  }
}

export async function fetchLeaderboard(
  page = 1,
  limit = 20,
  period?: string
): Promise<LeaderboardResponse> {
  const params = new URLSearchParams({ page: String(page), limit: String(limit) });
  if (period) params.set('period', period);
  try {
    const res = await fetch(`${API_URL}/api/leaderboard?${params}`);
    if (!res.ok) throw new Error('Failed to fetch leaderboard');
    const data = await res.json();
    // API returns { reports } but frontend expects { entries }
    return {
      entries: (data.reports || []).map((r: any) => ({
        id: r.id,
        repo_name: r.repo_name || r.github_username,
        ai_ratio: r.ai_ratio,
        grade: r.score_grade,
        score: r.score_points,
        roast: r.roast,
        created_at: r.created_at,
      })),
      total: data.total || 0,
      page: data.page || 1,
      limit: data.limit || 20,
    };
  } catch {
    return { entries: [], total: 0, page: 1, limit: 20 };
  }
}

export interface TrendPoint {
  month: string;      // "2025-06"
  avg_ai_ratio: number;
  total_scans: number;
  avg_score: number;
}

export interface TrendsResponse {
  period: string;
  trends: TrendPoint[];
}

export async function fetchTrends(period = '1y'): Promise<TrendsResponse> {
  try {
    const res = await fetch(`${API_URL}/api/trends?period=${period}`);
    if (!res.ok) throw new Error('Failed to fetch trends');
    return res.json();
  } catch {
    return { period, trends: [] };
  }
}

export interface ReportListEntry {
  id: string;
  github_username: string | null;
  repo_name: string | null;
  ai_ratio: number;
  score_points: number;
  score_grade: string;
  roast: string;
  created_at: string;
  updated_at: string | null;
}

export interface ReportsListResponse {
  reports: ReportListEntry[];
  total: number;
  page: number;
  limit: number;
}

export async function fetchReportsList(page = 1, limit = 20): Promise<ReportsListResponse> {
  try {
    const params = new URLSearchParams({ page: String(page), limit: String(limit) });
    const res = await fetch(`${API_URL}/api/reports/list?${params}`);
    if (!res.ok) throw new Error('Failed to fetch reports');
    return res.json();
  } catch {
    return { reports: [], total: 0, page: 1, limit: 20 };
  }
}

export function getApiUrl(): string {
  return API_URL;
}

export function gradeColor(grade: string): string {
  if (grade.startsWith('S')) return 'text-tokyo-green';
  if (grade.startsWith('A')) return 'text-tokyo-green';
  if (grade.startsWith('B')) return 'text-tokyo-cyan';
  if (grade.startsWith('C')) return 'text-tokyo-yellow';
  if (grade.startsWith('D')) return 'text-tokyo-red';
  if (grade === 'F') return 'text-red-500';
  return 'text-tokyo-text';
}

export function gradeBg(grade: string): string {
  if (grade.startsWith('S')) return 'bg-tokyo-green/20 border-tokyo-green/40';
  if (grade.startsWith('A')) return 'bg-tokyo-green/20 border-tokyo-green/40';
  if (grade.startsWith('B')) return 'bg-tokyo-cyan/20 border-tokyo-cyan/40';
  if (grade.startsWith('C')) return 'bg-tokyo-yellow/20 border-tokyo-yellow/40';
  if (grade.startsWith('D')) return 'bg-tokyo-red/20 border-tokyo-red/40';
  if (grade === 'F') return 'bg-red-500/20 border-red-500/40';
  return 'bg-tokyo-surface border-tokyo-border';
}
