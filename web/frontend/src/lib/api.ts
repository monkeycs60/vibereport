const API_URL = import.meta.env.PUBLIC_API_URL || 'https://api.vibereport.dev';

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
    return res.json();
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
    return res.json();
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
