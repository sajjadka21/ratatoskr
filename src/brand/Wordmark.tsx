import { useId } from "react";

/**
 * The RATATOSKR wordmark from the brand kit, with the O drawn as a runic
 * tree. The letters are vector paths, so no font is needed, and they take the
 * current text colour, so the wordmark follows every theme.
 */
export function Wordmark({ height = 28, className }: { height?: number; className?: string }) {
  const id = useId().replace(/:/g, "");
  const letter = (name: string) => `${id}-${name}`;
  const at = (x: number) => `translate(${x} 176) scale(0.087891 -0.087891)`;
  return (
    <svg
      role="img"
      aria-label="Ratatoskr"
      viewBox="0 0 1191 218"
      height={height}
      width={(height * 1191) / 218}
      className={className}
      fill="currentColor"
    >
      <defs>
        <path
          id={letter("r")}
          d="M1384 0L1022 0L563 634Q512 632 480 632Q467 632 452.0 632.5Q437 633 421 634L421 240Q421 112 449 81Q487 37 563 37L616 37L616 0L35 0L35 37L86 37Q172 37 209 93Q230 124 230 240L230 1116Q230 1244 202 1275Q163 1319 86 1319L35 1319L35 1356L529 1356Q745 1356 847.5 1324.5Q950 1293 1021.5 1208.5Q1093 1124 1093 1007Q1093 882 1011.5 790.0Q930 698 759 660L1039 271Q1135 137 1204.0 93.0Q1273 49 1384 37ZM421 697Q440 697 454.0 696.5Q468 696 477 696Q671 696 769.5 780.0Q868 864 868 994Q868 1121 788.5 1200.5Q709 1280 578 1280Q520 1280 421 1261Z"
        />
        <path
          id={letter("a")}
          d="M937 454L412 454L320 240Q286 161 286 122Q286 91 315.5 67.5Q345 44 443 37L443 0L16 0L16 37Q101 52 126 76Q177 124 239 271L716 1387L751 1387L1223 259Q1280 123 1326.5 82.5Q1373 42 1456 37L1456 0L921 0L921 37Q1002 41 1030.5 64.0Q1059 87 1059 120Q1059 164 1019 259ZM909 528L679 1076L443 528Z"
        />
        <path
          id={letter("t")}
          d="M1185 1356L1200 1038L1162 1038Q1151 1122 1132 1158Q1101 1216 1049.5 1243.5Q998 1271 914 1271L723 1271L723 235Q723 110 750 79Q788 37 867 37L914 37L914 0L339 0L339 37L387 37Q473 37 509 89Q531 121 531 235L531 1271L368 1271Q273 1271 233 1257Q181 1238 144.0 1184.0Q107 1130 100 1038L62 1038L78 1356Z"
        />
        <path
          id={letter("s")}
          d="M939 1387L939 918L902 918Q884 1053 837.5 1133.0Q791 1213 705.0 1260.0Q619 1307 527 1307Q423 1307 355.0 1243.5Q287 1180 287 1099Q287 1037 330 986Q392 911 625 786Q815 684 884.5 629.5Q954 575 991.5 501.0Q1029 427 1029 346Q1029 192 909.5 80.5Q790 -31 602 -31Q543 -31 491 -22Q460 -17 362.5 14.5Q265 46 239 46Q214 46 199.5 31.0Q185 16 178 -31L141 -31L141 434L178 434Q204 288 248.0 215.5Q292 143 382.5 95.0Q473 47 581 47Q706 47 778.5 113.0Q851 179 851 269Q851 319 823.5 370.0Q796 421 738 465Q699 495 525.0 592.5Q351 690 277.5 748.0Q204 806 166.0 876.0Q128 946 128 1030Q128 1176 240.0 1281.5Q352 1387 525 1387Q633 1387 754 1334Q810 1309 833 1309Q859 1309 875.5 1324.5Q892 1340 902 1387Z"
        />
        <path
          id={letter("k")}
          d="M612 752L1112 255Q1235 132 1322.0 87.5Q1409 43 1496 37L1496 0L851 0L851 37Q909 37 934.5 56.5Q960 76 960 100Q960 124 950.5 143.0Q941 162 888 214L420 677L420 240Q420 137 433 104Q443 79 475 61Q518 37 566 37L612 37L612 0L34 0L34 37L82 37Q166 37 204 86Q228 118 228 240L228 1116Q228 1219 215 1253Q205 1277 174 1295Q130 1319 82 1319L34 1319L34 1356L612 1356L612 1319L566 1319Q519 1319 475 1296Q444 1280 432.0 1248.0Q420 1216 420 1116L420 701Q440 720 557 828Q854 1100 916 1191Q943 1231 943 1261Q943 1284 922.0 1301.5Q901 1319 851 1319L820 1319L820 1356L1318 1356L1318 1319Q1274 1318 1238.0 1307.0Q1202 1296 1150.0 1264.5Q1098 1233 1022 1163Q1000 1143 819 958Z"
        />
      </defs>
      <use href={`#${letter("r")}`} transform={at(40)} />
      <use href={`#${letter("a")}`} transform={at(165.059)} />
      <use href={`#${letter("t")}`} transform={at(300.049)} />
      <use href={`#${letter("a")}`} transform={at(415)} />
      <use href={`#${letter("t")}`} transform={at(549.99)} />
      <g
        transform="translate(0 0)"
        stroke="currentColor"
        strokeWidth="3.575"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      >
        <path d="M727.94 50.22 C659.3 101.7 659.3 130.3 727.94 181.78 C796.58 130.3 796.58 101.7 727.94 50.22Z" />
        <path d="M727.94 70.24V157.47M705.2 87.4L727.94 108.85L750.68 87.4M700.06 108.85L727.94 131.73L755.83 108.85M727.94 144.6L705.2 160.33M727.94 144.6L750.68 160.33" />
      </g>
      <use href={`#${letter("s")}`} transform={at(795.941)} />
      <use href={`#${letter("k")}`} transform={at(901.049)} />
      <use href={`#${letter("r")}`} transform={at(1036.039)} />
    </svg>
  );
}

/** The runic tree from the wordmark's O, for small places and empty states. */
export function TreeMark({ size = 96, className }: { size?: number; className?: string }) {
  return (
    <svg
      viewBox="0 0 256 256"
      width={size}
      height={size}
      className={className}
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeWidth="5.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M128 26.8 C22.4 106 22.4 150 128 229.2 C233.6 150 233.6 106 128 26.8Z" />
      <path d="M128 57.6V191.8M93.02 84L128 117L162.98 84M85.1 117L128 152.2L170.9 117M128 172L93.02 196.2M128 172L162.98 196.2" />
    </svg>
  );
}
