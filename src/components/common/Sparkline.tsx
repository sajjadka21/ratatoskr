type SparklineProps = {
  values: number[];
  width: number;
  height: number;
  /** Fill under the line; used for the large throughput band. */
  area?: boolean;
  className?: string;
  /** Minimum top of the scale so a trickle does not look like a flood. */
  floor?: number;
};

/**
 * A tiny line chart. Oldest sample on the inline-start side, newest at the
 * end; the `sparkline` class mirrors it in right-to-left layouts so time
 * runs in reading direction there too.
 */
export function Sparkline({ values, width, height, area = false, className, floor = 1 }: SparklineProps) {
  if (values.length < 2) {
    return <svg className={`sparkline ${className ?? ""}`} width={width} height={height} aria-hidden="true" />;
  }
  const max = Math.max(floor, ...values);
  const step = width / (values.length - 1);
  const points = values.map((value, index) => [
    index * step,
    height - 1 - (Math.max(0, value) / max) * (height - 3),
  ]);
  const line = points.map(([x, y], index) => `${index ? "L" : "M"}${x.toFixed(1)} ${y.toFixed(1)}`).join("");
  const last = points[points.length - 1];

  return (
    <svg
      className={`sparkline ${className ?? ""}`}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      aria-hidden="true"
    >
      {area ? (
        <path d={`${line}L${width} ${height}L0 ${height}Z`} fill="currentColor" opacity="0.14" />
      ) : null}
      <path d={line} fill="none" stroke="currentColor" strokeWidth={area ? 2 : 1.5} strokeLinejoin="round" />
      <circle cx={last[0]} cy={last[1]} r={area ? 3 : 2} fill="currentColor" />
    </svg>
  );
}
