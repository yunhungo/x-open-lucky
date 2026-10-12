/**
 * @created 2026-10-13
 * @description 展示进程资源采样与容器限制并明确未知值。
 * @author yunhungo
 */
import { Cpu, MemoryStick } from 'lucide-react';
import { bytes, type Performance } from '@/lib/observability';
import './PerformancePanel.scss';
export function PerformancePanel({ data }: { data: Performance }) {
  const points = data.history.filter((p) => p.cpu_percent !== null);
  const max = Math.max(100, ...points.map((p) => p.cpu_percent ?? 0));
  const line = points
    .map(
      (p, i) =>
        `${(i / Math.max(points.length - 1, 1)) * 160},${30 - ((p.cpu_percent ?? 0) / max) * 28}`,
    )
    .join(' ');
  return (
    <section className='panel performance-panel' aria-label='性能'>
      <div className='performance-panel__metric'>
        <span title='OpenLucky 进程 CPU；一个 CPU 核心占满为 100%，每 5 秒采样'>
          <Cpu size={15} /> CPU
        </span>
        <strong>
          {data.cpu_percent === null ? '—' : `${data.cpu_percent.toFixed(1)}%`}
        </strong>
        {points.length > 1 && (
          <svg
            viewBox='0 0 160 32'
            preserveAspectRatio='none'
            role='img'
            aria-label='最近 10 分钟 CPU 趋势'
          >
            <polyline
              points={line}
              fill='none'
              stroke='var(--primary)'
              strokeWidth={1.5}
            />
          </svg>
        )}
      </div>
      <div className='performance-panel__metric'>
        <span title='OpenLucky 进程驻留内存 RSS'>
          <MemoryStick size={15} /> 进程内存
        </span>
        <strong>{bytes(data.memory_bytes)}</strong>
      </div>
      <div className='performance-panel__metric'>
        <span>容器内存</span>
        <strong>{bytes(data.container_memory_bytes)}</strong>
        {data.memory_limit_bytes !== null && (
          <small>/ {bytes(data.memory_limit_bytes)}</small>
        )}
      </div>
      {data.cpu_limit_cores !== null && (
        <div className='performance-panel__metric'>
          <span>CPU 上限</span>
          <strong>{data.cpu_limit_cores} 核</strong>
        </div>
      )}
      {data.memory_bytes === null && (
        <span className='performance-panel__unavailable'>
          当前系统不支持资源采样
        </span>
      )}
    </section>
  );
}
