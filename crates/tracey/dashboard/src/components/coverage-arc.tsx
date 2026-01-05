// SVG arc indicator for coverage progress

function CoverageArc({ count, total, color, title, size = 28 }: CoverageArcProps) {
  const bgStrokeWidth = 1.5;
  const fgStrokeWidth = 3;
  const radius = (size - fgStrokeWidth) / 2;
  const center = size / 2;
  const fraction = total > 0 ? count / total : 0;

  // Calculate arc path
  // Start at top (12 o'clock position), draw clockwise
  const startAngle = -Math.PI / 2;
  const endAngle = startAngle + fraction * 2 * Math.PI;

  // Calculate arc endpoints
  const startX = center + radius * Math.cos(startAngle);
  const startY = center + radius * Math.sin(startAngle);
  const endX = center + radius * Math.cos(endAngle);
  const endY = center + radius * Math.sin(endAngle);

  // Large arc flag: 1 if arc > 180 degrees
  const largeArcFlag = fraction > 0.5 ? 1 : 0;

  // SVG arc path
  const arcPath =
    fraction >= 1
      ? null // Use circle for full
      : fraction > 0
        ? `M ${startX} ${startY} A ${radius} ${radius} 0 ${largeArcFlag} 1 ${endX} ${endY}`
        : null;

  return html`
    <svg
      width=${size}
      height=${size}
      viewBox="0 0 ${size} ${size}"
      class="coverage-arc"
      style="flex-shrink: 0"
    >
      <title>${title}</title>
      <!-- Thin background ring (always full circle) -->
      <circle
        cx=${center}
        cy=${center}
        r=${radius}
        fill="none"
        stroke="var(--border)"
        stroke-width=${bgStrokeWidth}
      />
      <!-- Thick progress arc -->
      ${fraction >= 1
        ? html`
            <circle
              cx=${center}
              cy=${center}
              r=${radius}
              fill="none"
              stroke=${color}
              stroke-width=${fgStrokeWidth}
            />
          `
        : arcPath
          ? html`
              <path
                d=${arcPath}
                fill="none"
                stroke=${color}
                stroke-width=${fgStrokeWidth}
                stroke-linecap="round"
              />
            `
          : null}
      <!-- Centered text -->
      <text
        x=${center}
        y=${center}
        text-anchor="middle"
        dominant-baseline="central"
        fill="var(--fg-muted)"
        font-size="8"
        font-weight="500"
      >
        ${count}/${total}
      </text>
    </svg>
  `;
}
