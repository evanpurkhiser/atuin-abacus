import chroma from 'npm:chroma-js@2.6.0';

import type {DailyCommandCount} from './db.ts';

const DAYS = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];

const MONTHS = [
  'jan',
  'feb',
  'mar',
  'apr',
  'may',
  'jun',
  'jul',
  'aug',
  'sep',
  'oct',
  'nov',
  'dec',
];

export interface SvgOptions {
  cellSize?: number;
  cellGap?: number;
  showMonthLabels?: boolean;
  showDayLabels?: boolean;
  showFooter?: boolean;
  baseColor?: string;
  textColor?: string;
  cellBackground?: string;
}

interface Cell {
  date: string;
  count: number;
  x: number;
  y: number;
  intensity: number;
}

interface Dimensions {
  width: number;
  height: number;
  leftMargin: number;
  topMargin: number;
  graphWidth: number;
  graphHeight: number;
}

/**
 * Calculate intensity values for each date using logarithmic scale with percentile-based thresholds
 */
function calculateIntensityMap(data: DailyCommandCount[]): Map<string, number> {
  const intensityMap = new Map<string, number>();

  const nonZeroCounts = data
    .map(d => d.count)
    .filter(c => c > 0)
    .sort((a, b) => a - b);

  if (nonZeroCounts.length === 0) {
    data.forEach(item => intensityMap.set(item.date, 0));
    return intensityMap;
  }

  // Convert to log scale to handle wide range of values
  const logCounts = nonZeroCounts.map(c => Math.log10(c + 1));

  const getPercentile = (arr: number[], percentile: number): number => {
    const index = Math.ceil((percentile / 100) * arr.length) - 1;
    return arr[Math.max(0, index)];
  };

  // Get percentile thresholds on log scale
  const thresholds = {
    p10: getPercentile(logCounts, 10),
    p25: getPercentile(logCounts, 25),
    p40: getPercentile(logCounts, 40),
    p55: getPercentile(logCounts, 55),
    p70: getPercentile(logCounts, 70),
    p80: getPercentile(logCounts, 80),
    p88: getPercentile(logCounts, 88),
    p94: getPercentile(logCounts, 94),
    p98: getPercentile(logCounts, 98),
  };

  // Calculate intensity for each data point
  data.forEach(item => {
    if (item.count === 0) {
      intensityMap.set(item.date, 0);
      return;
    }

    const logCount = Math.log10(item.count + 1);
    let intensity: number;

    if (logCount <= thresholds.p10) {
      intensity = 1;
    } else if (logCount <= thresholds.p25) {
      intensity = 2;
    } else if (logCount <= thresholds.p40) {
      intensity = 3;
    } else if (logCount <= thresholds.p55) {
      intensity = 4;
    } else if (logCount <= thresholds.p70) {
      intensity = 5;
    } else if (logCount <= thresholds.p80) {
      intensity = 6;
    } else if (logCount <= thresholds.p88) {
      intensity = 7;
    } else if (logCount <= thresholds.p94) {
      intensity = 8;
    } else {
      intensity = 9;
    }

    intensityMap.set(item.date, intensity);
  });

  return intensityMap;
}

/**
 * Generate a continuous grid of cells from first Sunday through last date
 */
function generateCells(
  data: DailyCommandCount[],
  intensityMap: Map<string, number>,
  cellSize: number,
  cellGap: number
): Cell[] {
  // Start from the first Sunday on or before the first data point
  const firstDate = new Date(data[0].date);
  const startDate = new Date(firstDate);
  const firstDayOfWeek = startDate.getDay();
  if (firstDayOfWeek !== 0) {
    startDate.setDate(startDate.getDate() - firstDayOfWeek);
  }

  const lastDate = new Date(data[data.length - 1].date);
  const dataMap = new Map(data.map(d => [d.date, d]));

  const cells: Cell[] = [];
  let weekIndex = 0;
  const currentDate = new Date(startDate);

  while (currentDate <= lastDate) {
    const dateStr = currentDate.toISOString().split('T')[0];
    const dayOfWeek = currentDate.getDay();

    const dataPoint = dataMap.get(dateStr);
    const count = dataPoint?.count ?? 0;
    const intensity = intensityMap.get(dateStr) ?? 0;

    cells.push({
      date: dateStr,
      count,
      x: weekIndex * (cellSize + cellGap),
      y: dayOfWeek * (cellSize + cellGap),
      intensity,
    });

    if (dayOfWeek === 6) {
      weekIndex++;
    }

    currentDate.setDate(currentDate.getDate() + 1);
  }

  return cells;
}

/**
 * Calculate SVG dimensions based on cells and options
 */
function calculateDimensions(
  cells: Cell[],
  cellSize: number,
  cellGap: number,
  showDayLabels: boolean,
  showMonthLabels: boolean,
  showFooter: boolean
): Dimensions {
  const maxX = Math.max(...cells.map(c => c.x));
  const leftMargin = showDayLabels ? 30 : 10;
  const topMargin = showMonthLabels ? 20 : 10;
  const rightMargin = 10;
  const bottomMargin = showFooter ? 35 : 10;

  const graphWidth = maxX + cellSize + cellGap;
  const graphHeight = 7 * (cellSize + cellGap);

  return {
    width: leftMargin + graphWidth + rightMargin,
    height: topMargin + graphHeight + bottomMargin,
    leftMargin,
    topMargin,
    graphWidth,
    graphHeight,
  };
}

/**
 * Render day of week labels
 */
function renderDayLabels(
  dims: Dimensions,
  cellSize: number,
  cellGap: number,
  textColor: string
): string {
  let svg = '';

  DAYS.forEach((day, i) => {
    const y = dims.topMargin + i * (cellSize + cellGap) + cellSize / 2;
    const x = dims.leftMargin - 18;
    svg += `<text x="${x}" y="${y}" fill="${textColor}" font-size="10" font-family="monospace" dominant-baseline="middle">${day}</text>`;
  });

  return svg;
}

/**
 * Render month labels
 */
function renderMonthLabels(cells: Cell[], leftMargin: number, textColor: string): string {
  let currentMonth = -1;
  const monthPositions: Array<{month: string; x: number; day: number}> = [];

  cells.forEach(cell => {
    const date = new Date(cell.date);
    const month = date.getMonth();
    const day = date.getDate();

    if (month !== currentMonth) {
      currentMonth = month;
      monthPositions.push({
        month: MONTHS[month],
        x: leftMargin + cell.x,
        day,
      });
    }
  });

  // Skip the first month label if it doesn't start near the beginning of the month
  // This prevents overlapping labels when the graph starts mid-month
  if (monthPositions.length > 0 && monthPositions[0].day > 7) {
    monthPositions.shift();
  }

  return monthPositions
    .map(
      pos =>
        `<text x="${pos.x}" y="12" fill="${textColor}" font-size="10" font-family="monospace">${pos.month}</text>`
    )
    .join('');
}

/**
 * Render cell rectangles
 */
function renderCells(
  cells: Cell[],
  leftMargin: number,
  topMargin: number,
  cellSize: number,
  getColor: (intensity: number) => string
): string {
  return cells
    .map(cell => {
      const color = getColor(cell.intensity);
      const x = leftMargin + cell.x;
      const y = topMargin + cell.y;

      return `<rect x="${x}" y="${y}" width="${cellSize}" height="${cellSize}" fill="${color}" rx="2"/>`;
    })
    .join('\n');
}

/**
 * Render month separator lines that weave through the grid
 */
function renderMonthSeparators(
  cells: Cell[],
  leftMargin: number,
  topMargin: number,
  cellSize: number,
  cellGap: number,
  textColor: string
): string {
  if (cells.length === 0) {
    return '';
  }

  // Create a map of (x, y) -> cell for easy lookup
  const cellMap = new Map<string, Cell>();
  cells.forEach(cell => {
    cellMap.set(`${cell.x},${cell.y}`, cell);
  });

  // Find unique month boundaries by checking which columns have month changes
  const monthKeys = new Set<string>();
  cells.forEach(cell => {
    const date = new Date(cell.date);
    monthKeys.add(`${date.getFullYear()}-${date.getMonth()}`);
  });

  if (monthKeys.size < 2) {
    return '';
  }

  // Group cells by month
  const monthCells = new Map<string, Cell[]>();
  cells.forEach(cell => {
    const date = new Date(cell.date);
    const key = `${date.getFullYear()}-${date.getMonth()}`;
    if (!monthCells.has(key)) {
      monthCells.set(key, []);
    }
    monthCells.get(key)!.push(cell);
  });

  const sortedMonths = Array.from(monthCells.keys()).sort();
  const paths: string[] = [];
  const cellBorderRadius = 2;
  const radius = cellBorderRadius + cellGap / 2;

  // Convert textColor to 75% opacity
  const strokeColor = textColor.startsWith('#')
    ? `${textColor}C0` // Add C0 hex (75% opacity) to hex colors
    : textColor.replace('rgb(', 'rgba(').replace(')', ', 0.75)');

  // For each pair of consecutive months, draw a separator
  for (let i = 0; i < sortedMonths.length - 1; i++) {
    const prevMonthCells = monthCells.get(sortedMonths[i])!;
    const currMonthCells = monthCells.get(sortedMonths[i + 1])!;

    const path: string[] = [];
    let currentX: number | null = null;

    // Start from top of graph
    const startY = topMargin;

    // For each row (0-6), determine which column the boundary is in
    for (let row = 0; row < 7; row++) {
      const y = row * (cellSize + cellGap);

      // Find cells in this row for both months
      const prevInRow = prevMonthCells.filter(c => c.y === y);
      const currInRow = currMonthCells.filter(c => c.y === y);

      if (prevInRow.length === 0 && currInRow.length === 0) {
        continue;
      }

      // Determine the X position for this row
      let targetX: number;

      if (prevInRow.length > 0 && currInRow.length > 0) {
        // Both months have cells in this row
        // Separator should be between the rightmost prev cell and leftmost curr cell
        const maxPrevX = Math.max(...prevInRow.map(c => c.x));
        const minCurrX = Math.min(...currInRow.map(c => c.x));

        if (maxPrevX === minCurrX) {
          // Same column - separator in the middle
          targetX = leftMargin + maxPrevX + cellSize + cellGap / 2;
        } else {
          // Different columns - separator at the gap between columns
          targetX = leftMargin + maxPrevX + cellSize + cellGap / 2;
        }
      } else if (prevInRow.length > 0) {
        // Only prev month in this row
        const maxPrevX = Math.max(...prevInRow.map(c => c.x));
        targetX = leftMargin + maxPrevX + cellSize + cellGap / 2;
      } else {
        // Only curr month in this row
        const minCurrX = Math.min(...currInRow.map(c => c.x));
        targetX = leftMargin + minCurrX - cellGap / 2;
      }

      // Initialize or update path
      if (currentX === null) {
        // First point
        path.push(`M ${targetX} ${startY}`);
        currentX = targetX;
      } else if (currentX !== targetX) {
        // Need to step horizontally
        const stepY = topMargin + y - cellGap / 2;

        // Go down to step point
        path.push(`L ${currentX} ${stepY - radius}`);

        // Curve and step horizontally
        const direction = targetX > currentX ? 1 : -1;
        path.push(`Q ${currentX} ${stepY} ${currentX + direction * radius} ${stepY}`);
        path.push(`L ${targetX - direction * radius} ${stepY}`);
        path.push(`Q ${targetX} ${stepY} ${targetX} ${stepY + radius}`);

        currentX = targetX;
      }
    }

    // End at bottom of graph
    if (currentX !== null) {
      // Bottom should be at the end of the last row (row 6)
      const bottomY = topMargin + 6 * (cellSize + cellGap) + cellSize;
      path.push(`L ${currentX} ${bottomY}`);
      paths.push(
        `<path d="${path.join(
          ' '
        )}" fill="none" stroke="${strokeColor}" stroke-width="0.5" stroke-linecap="round"/>`
      );
    }
  }

  return paths.join('\n');
}

/**
 * Render footer with color legend and statistics
 */
function renderFooter(
  data: DailyCommandCount[],
  dims: Dimensions,
  textColor: string,
  getColor: (intensity: number) => string
): string {
  const footerY = dims.topMargin + dims.graphHeight + 18;
  const legendSquareSize = 10;
  const legendGap = 3;

  // Calculate total commands and days
  const totalCommands = data.reduce((sum, d) => sum + d.count, 0);
  const totalDays = data.length;

  let svg = '';

  // Legend on the left
  const legendX = dims.leftMargin;

  // "Less" label
  svg += `<text x="${legendX}" y="${footerY}" fill="${textColor}" font-size="11" font-family="monospace" dominant-baseline="middle">Less</text>`;

  // Color squares - show intensity levels 0, 1, 3, 5, 6, 7, 8, 9
  const intensities = [0, 1, 3, 5, 6, 7, 8, 9];
  const squaresStartX = legendX + 35;

  intensities.forEach((intensity, i) => {
    const x = squaresStartX + i * (legendSquareSize + legendGap);
    const y = footerY - legendSquareSize / 2;
    const color = getColor(intensity);
    svg += `<rect x="${x}" y="${y}" width="${legendSquareSize}" height="${legendSquareSize}" fill="${color}" rx="2"/>`;
  });

  // "More" label
  const moreX = squaresStartX + intensities.length * (legendSquareSize + legendGap) + 5;
  svg += `<text x="${moreX}" y="${footerY}" fill="${textColor}" font-size="11" font-family="monospace" dominant-baseline="middle">More</text>`;

  // Statistics on the right
  const statsText = `${totalCommands.toLocaleString()} commands over ${totalDays} days`;
  const statsX = dims.width - 10;
  svg += `<text x="${statsX}" y="${footerY}" fill="${textColor}" font-size="11" font-family="monospace" dominant-baseline="middle" text-anchor="end">${statsText}</text>`;

  return svg;
}

/**
 * Generate a GitHub-style contribution graph SVG from daily command counts
 */
export function generateContributionGraph(
  data: DailyCommandCount[],
  options: SvgOptions = {}
): string {
  const {
    cellSize = 12,
    cellGap = 3,
    showMonthLabels = true,
    showDayLabels = true,
    showFooter = true,
    baseColor = '#fb7185',
    textColor = '#57606a',
    cellBackground = '#ebedf0',
  } = options;

  // If no data, generate a year's worth of empty cells
  let processedData = data;
  if (data.length === 0) {
    const endDate = new Date();
    const startDate = new Date(endDate);
    startDate.setFullYear(startDate.getFullYear() - 1);

    processedData = [];
    const currentDate = new Date(startDate);
    while (currentDate <= endDate) {
      processedData.push({
        date: currentDate.toISOString().split('T')[0],
        count: 0,
      });
      currentDate.setDate(currentDate.getDate() + 1);
    }
  }

  const intensityMap = calculateIntensityMap(processedData);
  const cells = generateCells(processedData, intensityMap, cellSize, cellGap);
  const dims = calculateDimensions(
    cells,
    cellSize,
    cellGap,
    showDayLabels,
    showMonthLabels,
    showFooter
  );

  const getColor = createColorScale(baseColor, cellBackground);

  let svg = `<svg width="${dims.width}" height="${dims.height}" xmlns="http://www.w3.org/2000/svg">`;

  if (showDayLabels) {
    svg += renderDayLabels(dims, cellSize, cellGap, textColor);
  }

  if (showMonthLabels) {
    svg += renderMonthLabels(cells, dims.leftMargin, textColor);
  }

  svg += renderCells(cells, dims.leftMargin, dims.topMargin, cellSize, getColor);
  svg += renderMonthSeparators(
    cells,
    dims.leftMargin,
    dims.topMargin,
    cellSize,
    cellGap,
    textColor
  );

  if (showFooter) {
    svg += renderFooter(processedData, dims, textColor, getColor);
  }

  svg += '</svg>';

  return svg;
}

/**
 * Generate a perceptually uniform color scale with proper visual contrast
 * Uses lightness correction for even perceptual steps
 */
function createColorScale(
  baseColor: string,
  cellBackground: string
): (intensity: number) => string {
  // Create a desaturated, lighter starting point (not quite the background)
  const base = chroma(baseColor);
  const lightStart = base.set('hsl.s', '*0.2').set('hsl.l', '*1.3');

  // Generate scale with lightness correction for even perceptual steps
  // This ensures each step has similar visual impact
  const colors = chroma
    .scale([lightStart, baseColor])
    .mode('lab')
    .correctLightness()
    .colors(8); // 8 colors for intensities 0-7

  // Create two brighter levels above the base
  const bright1 = base.brighten(0.6).saturate(0.3);
  const bright2 = base.brighten(1.2).saturate(0.5);

  return (intensity: number): string => {
    if (intensity === 0) {
      return cellBackground;
    }

    if (intensity <= 7) {
      return colors[intensity];
    }
    if (intensity === 8) {
      return bright1.hex();
    }
    // intensity === 9
    return bright2.hex();
  };
}
