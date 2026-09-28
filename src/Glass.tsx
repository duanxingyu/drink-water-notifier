import { useId } from "react";

type GlassProps = {
  dismissSeconds?: number;
};

export function Glass({ dismissSeconds = 20 }: GlassProps) {
  const uid = useId().replace(/:/g, "");
  const clip = `water-${uid}`;
  const wave = `M0 24 Q27.5 4 55 24 T110 24 T165 24 T220 24 T275 24 T330 24 T385 24 T440 24 V150 H0 Z`;

  return (
    <svg viewBox="0 0 220 250" aria-hidden="true">
      <ellipse cx="110" cy="214" rx="58" ry="8" fill="rgba(28,43,42,0.08)" />
      <circle
        className="countdown-ring"
        cx="110"
        cy="132"
        r="96"
        fill="none"
        stroke="#e7d3b4"
        strokeWidth="3"
        style={{ animationDuration: `${dismissSeconds}s` }}
      />
      <g className="glass-bob">
        <g className="drop" fill="#14968c">
          <path d="M110 18c0 0 11 16 11 24a11 11 0 1 1-22 0c0-8 11-24 11-24z" />
        </g>
        <defs>
          <clipPath id={clip}>
            <path d="M74 86 L84 188 Q110 206 136 188 L146 86 Q110 96 74 86 Z" />
          </clipPath>
        </defs>
        <path
          d="M70 84 Q110 98 150 84 L140 190 Q110 212 80 190 Z"
          fill="rgba(255,255,255,0.38)"
          stroke="#1d4743"
          strokeWidth="3"
        />
        <g clipPath={`url(#${clip})`}>
          <g className="water">
            <g className="waves">
              <path d={wave} fill="#17867e" transform="translate(-20 92)" />
              <path d={wave} fill="#bffff4" opacity="0.45" transform="translate(-8 100)" />
            </g>
            <circle className="bubble b1" cx="96" cy="176" r="3.5" fill="rgba(255,255,255,0.75)" />
            <circle className="bubble b2" cx="122" cy="184" r="2.4" fill="rgba(255,255,255,0.7)" />
            <circle className="bubble b3" cx="108" cy="190" r="2" fill="rgba(255,255,255,0.65)" />
          </g>
        </g>
        <path
          d="M84 96c6 8 8 28 4 52"
          fill="none"
          stroke="rgba(255,255,255,0.7)"
          strokeWidth="4"
          strokeLinecap="round"
        />
        <ellipse cx="110" cy="86" rx="38" ry="8" fill="none" stroke="#1d4743" strokeWidth="3" />
      </g>
    </svg>
  );
}
