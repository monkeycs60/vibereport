import type { APIRoute } from 'astro';
import { ImageResponse } from '@vercel/og';
import { fetchReport } from '../../lib/api';

// Grade → hex color (Tokyo Night palette)
function gradeHex(grade: string): string {
  if (grade.startsWith('S') || grade.startsWith('A')) return '#9ece6a';
  if (grade.startsWith('B')) return '#7aa2f7';
  if (grade.startsWith('C')) return '#e0af68';
  if (grade.startsWith('D') || grade === 'F') return '#f7768e';
  return '#c0caf5';
}

// Module-level font cache
let fontData: ArrayBuffer | null = null;
async function loadFont(): Promise<ArrayBuffer> {
  if (fontData) return fontData;
  const res = await fetch(
    'https://cdn.jsdelivr.net/gh/JetBrains/JetBrainsMono@2.304/fonts/ttf/JetBrainsMono-Bold.ttf'
  );
  fontData = await res.arrayBuffer();
  return fontData;
}

export const GET: APIRoute = async ({ url }) => {
  const id = url.searchParams.get('id');
  const font = await loadFont();

  // Default card when no id or report not found
  let repoName = 'vibereport';
  let grade = '';
  let aiPct = '';
  let score = '';
  let totalCommits = '';
  let roast = 'How much of your code is AI-generated?';
  let aiRatio = 0;
  let hasReport = false;

  if (id) {
    const report = await fetchReport(id);
    if (report) {
      hasReport = true;
      repoName = report.repo_name || 'unknown';
      grade = report.grade || '?';
      aiPct = `${Math.round((report.ai_ratio || 0) * 100)}%`;
      score = String(report.score ?? 0);
      totalCommits = String(report.total_commits ?? 0);
      roast = report.roast || '';
      aiRatio = report.ai_ratio || 0;
    }
  }

  const gradeColor = grade ? gradeHex(grade) : '#7aa2f7';
  const barWidth = Math.round(aiRatio * 100);

  const element = hasReport
    ? // Per-report card
      {
        type: 'div',
        props: {
          style: {
            display: 'flex',
            flexDirection: 'column',
            width: '100%',
            height: '100%',
            backgroundColor: '#1a1b26',
            padding: '50px 60px',
            fontFamily: 'JetBrains Mono',
          },
          children: [
            // Top row: branding + grade
            {
              type: 'div',
              props: {
                style: {
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                },
                children: [
                  {
                    type: 'span',
                    props: {
                      style: { fontSize: 28, color: '#7dcfff' },
                      children: 'vibereport',
                    },
                  },
                  {
                    type: 'span',
                    props: {
                      style: {
                        fontSize: 52,
                        fontWeight: 700,
                        color: gradeColor,
                      },
                      children: grade,
                    },
                  },
                ],
              },
            },
            // Repo name
            {
              type: 'div',
              props: {
                style: {
                  display: 'flex',
                  justifyContent: 'center',
                  marginTop: 30,
                },
                children: {
                  type: 'span',
                  props: {
                    style: {
                      fontSize: 36,
                      fontWeight: 700,
                      color: '#c0caf5',
                    },
                    children: repoName,
                  },
                },
              },
            },
            // Stats row
            {
              type: 'div',
              props: {
                style: {
                  display: 'flex',
                  justifyContent: 'center',
                  gap: 80,
                  marginTop: 40,
                },
                children: [
                  {
                    type: 'div',
                    props: {
                      style: {
                        display: 'flex',
                        flexDirection: 'column',
                        alignItems: 'center',
                      },
                      children: [
                        {
                          type: 'span',
                          props: {
                            style: {
                              fontSize: 48,
                              fontWeight: 700,
                              color: '#f7768e',
                            },
                            children: aiPct,
                          },
                        },
                        {
                          type: 'span',
                          props: {
                            style: {
                              fontSize: 16,
                              color: '#565f89',
                              marginTop: 4,
                            },
                            children: 'AI commits',
                          },
                        },
                      ],
                    },
                  },
                  {
                    type: 'div',
                    props: {
                      style: {
                        display: 'flex',
                        flexDirection: 'column',
                        alignItems: 'center',
                      },
                      children: [
                        {
                          type: 'span',
                          props: {
                            style: {
                              fontSize: 48,
                              fontWeight: 700,
                              color: gradeColor,
                            },
                            children: `Score ${score}`,
                          },
                        },
                        {
                          type: 'span',
                          props: {
                            style: {
                              fontSize: 16,
                              color: '#565f89',
                              marginTop: 4,
                            },
                            children: 'Vibe Score',
                          },
                        },
                      ],
                    },
                  },
                  {
                    type: 'div',
                    props: {
                      style: {
                        display: 'flex',
                        flexDirection: 'column',
                        alignItems: 'center',
                      },
                      children: [
                        {
                          type: 'span',
                          props: {
                            style: {
                              fontSize: 48,
                              fontWeight: 700,
                              color: '#c0caf5',
                            },
                            children: totalCommits,
                          },
                        },
                        {
                          type: 'span',
                          props: {
                            style: {
                              fontSize: 16,
                              color: '#565f89',
                              marginTop: 4,
                            },
                            children: 'commits',
                          },
                        },
                      ],
                    },
                  },
                ],
              },
            },
            // Roast
            {
              type: 'div',
              props: {
                style: {
                  display: 'flex',
                  justifyContent: 'center',
                  marginTop: 36,
                },
                children: {
                  type: 'span',
                  props: {
                    style: {
                      fontSize: 22,
                      color: '#e0af68',
                      fontStyle: 'italic',
                    },
                    children: `"${roast}"`,
                  },
                },
              },
            },
            // AI ratio bar + footer
            {
              type: 'div',
              props: {
                style: {
                  display: 'flex',
                  flexDirection: 'column',
                  marginTop: 'auto',
                  gap: 12,
                },
                children: [
                  // Bar background
                  {
                    type: 'div',
                    props: {
                      style: {
                        display: 'flex',
                        width: '100%',
                        height: 16,
                        backgroundColor: '#24283b',
                        borderRadius: 8,
                        overflow: 'hidden',
                      },
                      children: {
                        type: 'div',
                        props: {
                          style: {
                            width: `${barWidth}%`,
                            height: '100%',
                            background:
                              'linear-gradient(90deg, #f7768e, #e0af68)',
                            borderRadius: 8,
                          },
                        },
                      },
                    },
                  },
                  // Footer
                  {
                    type: 'div',
                    props: {
                      style: {
                        display: 'flex',
                        justifyContent: 'flex-end',
                      },
                      children: {
                        type: 'span',
                        props: {
                          style: { fontSize: 16, color: '#565f89' },
                          children: 'vibereport.dev',
                        },
                      },
                    },
                  },
                ],
              },
            },
          ],
        },
      }
    : // Default branded card (no report)
      {
        type: 'div',
        props: {
          style: {
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            width: '100%',
            height: '100%',
            backgroundColor: '#1a1b26',
            fontFamily: 'JetBrains Mono',
            gap: 20,
          },
          children: [
            {
              type: 'span',
              props: {
                style: { fontSize: 56, fontWeight: 700, color: '#7dcfff' },
                children: 'vibereport',
              },
            },
            {
              type: 'span',
              props: {
                style: { fontSize: 24, color: '#c0caf5' },
                children: 'How much of your code is AI-generated?',
              },
            },
            {
              type: 'span',
              props: {
                style: { fontSize: 18, color: '#565f89', marginTop: 10 },
                children: 'vibereport.dev',
              },
            },
          ],
        },
      };

  return new ImageResponse(element as any, {
    width: 1200,
    height: 630,
    fonts: [
      {
        name: 'JetBrains Mono',
        data: font,
        weight: 700,
        style: 'normal',
      },
    ],
    headers: {
      'Cache-Control': 'public, max-age=86400, s-maxage=86400',
    },
  });
};
