/**
 * @created 2026-10-13
 * @description 绘制紧凑且可读出具体时间与访问量的柱形趋势。
 * @author yunhungo
 */
import { useState } from 'react';
import { dateTime } from '@/lib/contracts';
import { number, type Traffic } from '@/lib/observability';
export function TrafficChart({ series }: { series: Traffic['series'] }) {
  const [selectedTime, setSelectedTime] = useState<number | null>(null);
  // 较长范围按天展示；API 的小时聚合保留准确总量。
  const stride = series.length > 168 ? 24 : series.length > 24 ? 6 : 1;
  const points = series
    .filter((_, i) => i % stride === 0)
    .map((row, i) => ({
      time: row.time,
      requests: series
        .slice(i * stride, (i + 1) * stride)
        .reduce((n, r) => n + r.requests, 0),
      errors: series
        .slice(i * stride, (i + 1) * stride)
        .reduce((n, r) => n + r.server_errors, 0),
    }));
  const max = Math.max(1, ...points.map((p) => p.requests));
  const selected = points.find((p) => p.time === selectedTime);
  const activeTime = selected?.time ?? points.at(-1)?.time;
  return (
    <div className='traffic-chart'>
      <div className='traffic-chart__scale'>
        <span>{number(max)}</span>
        <span>请求</span>
      </div>
      <svg
        viewBox={`0 0 ${points.length * 12} 80`}
        preserveAspectRatio='none'
        role='group'
        aria-label='访问趋势，绿色为请求，红色为服务端错误'
      >
        {points.map((p, i) => (
          <g
            key={p.time}
            role='button'
            data-index={i}
            tabIndex={p.time === activeTime ? 0 : -1}
            aria-pressed={p.time === selectedTime}
            aria-label={`${dateTime(p.time)} · ${number(p.requests)} 请求 · ${p.errors} 服务端错误`}
            onFocus={() => setSelectedTime(p.time)}
            onClick={() => setSelectedTime(p.time)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                setSelectedTime(p.time);
              }
              if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
                event.preventDefault();
                const next =
                  (i + (event.key === 'ArrowRight' ? 1 : -1) + points.length) %
                  points.length;
                const point = points[next];
                if (point) setSelectedTime(point.time);
                event.currentTarget.parentElement
                  ?.querySelector<SVGGElement>(`g[data-index="${next}"]`)
                  ?.focus();
              }
            }}
          >
            <rect
              x={i * 12}
              y={0}
              width={12}
              height={80}
              fill={p.time === selectedTime ? 'var(--muted)' : 'transparent'}
            />
            <title>
              {dateTime(p.time)} · {number(p.requests)} 请求 · {p.errors}{' '}
              服务端错误
            </title>
            <rect
              x={i * 12 + 1}
              y={80 - (p.requests / max) * 76}
              width={10}
              height={Math.max(0.5, (p.requests / max) * 76)}
              rx={1}
              fill='var(--primary)'
              opacity={0.65}
            />
            {p.errors > 0 && (
              <rect
                x={i * 12 + 1}
                y={80 - (p.errors / max) * 76}
                width={10}
                height={(p.errors / max) * 76}
                rx={1}
                fill='var(--destructive)'
              />
            )}
          </g>
        ))}
      </svg>
      {selected && (
        <div className='traffic-chart__selection' role='status'>
          <span>{dateTime(selected.time)}</span>
          <span>
            {number(selected.requests)} 请求 · {selected.errors} 服务端错误
          </span>
        </div>
      )}
      <div className='traffic-chart__axis'>
        <span>{dateTime(series[0]?.time)}</span>
        <span>{dateTime(series.at(-1)?.time)}</span>
      </div>
    </div>
  );
}
