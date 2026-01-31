import {assert, assertEquals, assertExists} from '@std/assert';

import type {DailyCommandCount} from './db.ts';
import {generateContributionGraph} from './svg.ts';

// Helper to extract SVG dimensions
function getSvgDimensions(svg: string): {width: number; height: number} {
  const widthMatch = svg.match(/width="(\d+)"/);
  const heightMatch = svg.match(/height="(\d+)"/);
  return {
    width: widthMatch ? parseInt(widthMatch[1], 10) : 0,
    height: heightMatch ? parseInt(heightMatch[1], 10) : 0,
  };
}

// Helper to count elements in SVG
function countElements(svg: string, element: string): number {
  const regex = new RegExp(`<${element}[^>]*>`, 'g');
  const matches = svg.match(regex);
  return matches ? matches.length : 0;
}

Deno.test('generateContributionGraph with empty data generates grid', () => {
  const svg = generateContributionGraph([]);

  // Should generate a valid SVG with a grid (not just "No data" text)
  assert(svg.startsWith('<svg'));
  assert(svg.endsWith('</svg>'));

  // Should have rect elements for the empty grid (approximately a year's worth)
  const rectCount = countElements(svg, 'rect');
  assert(rectCount >= 365); // At least 365 days
});

Deno.test('generateContributionGraph empty data respects options', () => {
  const svg = generateContributionGraph([], {
    textColor: '#ff00ff',
    cellBackground: '#000000',
    showMonthLabels: true,
  });

  // Should have a grid with custom colors
  assert(svg.includes('#000000')); // cellBackground color for empty cells
  assert(svg.includes('#ff00ff')); // textColor for labels

  // Should have month labels
  const hasMonthLabel = svg.includes('jan') || svg.includes('feb') || svg.includes('mar');
  assert(hasMonthLabel);
});

Deno.test('generateContributionGraph generates valid SVG', () => {
  const data: DailyCommandCount[] = [
    {date: '2024-01-01', count: 10},
    {date: '2024-01-02', count: 20},
    {date: '2024-01-03', count: 15},
  ];

  const svg = generateContributionGraph(data);

  // Should start with SVG tag
  assert(svg.startsWith('<svg'));
  // Should end with closing SVG tag
  assert(svg.endsWith('</svg>'));
  // Should have width and height attributes
  assertExists(svg.match(/width="\d+"/));
  assertExists(svg.match(/height="\d+"/));
});

Deno.test('generateContributionGraph creates rect for each cell', () => {
  const data: DailyCommandCount[] = [
    {date: '2024-01-01', count: 10},
    {date: '2024-01-02', count: 20},
    {date: '2024-01-03', count: 15},
  ];

  const svg = generateContributionGraph(data);

  // Should have rect elements (at least one per data point, possibly more due to grid layout)
  const rectCount = countElements(svg, 'rect');
  assert(rectCount >= data.length);
});

Deno.test('generateContributionGraph respects cellSize option', () => {
  const data: DailyCommandCount[] = [{date: '2024-01-01', count: 10}];

  const svg1 = generateContributionGraph(data, {cellSize: 10});
  const svg2 = generateContributionGraph(data, {cellSize: 20});

  const dims1 = getSvgDimensions(svg1);
  const dims2 = getSvgDimensions(svg2);

  // Larger cellSize should result in larger SVG
  assert(dims2.width > dims1.width || dims2.height > dims1.height);
});

Deno.test('generateContributionGraph respects showMonthLabels option', () => {
  const data: DailyCommandCount[] = [
    {date: '2024-01-01', count: 10},
    {date: '2024-02-01', count: 20},
  ];

  const svgWithLabels = generateContributionGraph(data, {showMonthLabels: true});
  const svgWithoutLabels = generateContributionGraph(data, {showMonthLabels: false});

  // With labels should have text elements for months
  assert(svgWithLabels.includes('jan') || svgWithLabels.includes('feb'));
  // Without labels should not have month names
  assert(!svgWithoutLabels.includes('jan'));
  assert(!svgWithoutLabels.includes('feb'));
});

Deno.test('generateContributionGraph respects showDayLabels option', () => {
  const data: DailyCommandCount[] = [
    {date: '2024-01-01', count: 10},
    {date: '2024-01-08', count: 20},
  ];

  const svgWithLabels = generateContributionGraph(data, {showDayLabels: true});
  const svgWithoutLabels = generateContributionGraph(data, {showDayLabels: false});

  // With labels should have more text elements
  const textCountWith = countElements(svgWithLabels, 'text');
  const textCountWithout = countElements(svgWithoutLabels, 'text');
  assert(textCountWith > textCountWithout);
});

Deno.test('generateContributionGraph respects baseColor option', () => {
  const data: DailyCommandCount[] = [{date: '2024-01-01', count: 100}];

  const svgDefault = generateContributionGraph(data);
  const svgCustom = generateContributionGraph(data, {baseColor: '#ff0000'});

  // SVGs with different base colors should be different
  assert(svgDefault !== svgCustom);
  // Custom color SVG should contain the custom color or colors derived from it
  assert(svgCustom.includes('fill='));
});

Deno.test('generateContributionGraph respects textColor option', () => {
  const data: DailyCommandCount[] = [{date: '2024-01-01', count: 10}];

  const svgCustom = generateContributionGraph(data, {textColor: '#ffffff'});

  // Should contain the custom text color
  assert(svgCustom.includes('#ffffff'));
});

Deno.test('generateContributionGraph respects cellBackground option', () => {
  const data: DailyCommandCount[] = [
    {date: '2024-01-01', count: 0}, // Zero count should use cellBackground
  ];

  const svgCustom = generateContributionGraph(data, {cellBackground: '#000000'});

  // Should contain the custom cell background color
  assert(svgCustom.includes('#000000'));
});

Deno.test('generateContributionGraph handles single day', () => {
  const data: DailyCommandCount[] = [{date: '2024-01-01', count: 42}];

  const svg = generateContributionGraph(data);

  assert(svg.includes('<svg'));
  assert(svg.includes('</svg>'));
  const rectCount = countElements(svg, 'rect');
  assert(rectCount >= 1);
});

Deno.test('generateContributionGraph handles year of data', () => {
  // Generate a year of data
  const data: DailyCommandCount[] = [];
  const startDate = new Date('2024-01-01');
  for (let i = 0; i < 365; i++) {
    const date = new Date(startDate);
    date.setDate(startDate.getDate() + i);
    data.push({
      date: date.toISOString().split('T')[0],
      count: Math.floor(Math.random() * 100),
    });
  }

  const svg = generateContributionGraph(data);

  assert(svg.includes('<svg'));
  assert(svg.includes('</svg>'));
  const rectCount = countElements(svg, 'rect');
  // Should have at least 365 rects (one per day)
  assert(rectCount >= 365);
});

Deno.test('generateContributionGraph handles varying counts', () => {
  const data: DailyCommandCount[] = [
    {date: '2024-01-01', count: 0},
    {date: '2024-01-02', count: 1},
    {date: '2024-01-03', count: 10},
    {date: '2024-01-04', count: 100},
    {date: '2024-01-05', count: 1000},
  ];

  const svg = generateContributionGraph(data);

  // Should generate valid SVG with different fill colors for different intensities
  assert(svg.includes('<svg'));
  const rectCount = countElements(svg, 'rect');
  assert(rectCount >= data.length);
});

Deno.test('generateContributionGraph with all options', () => {
  const data: DailyCommandCount[] = [
    {date: '2024-01-01', count: 10},
    {date: '2024-01-02', count: 20},
  ];

  const svg = generateContributionGraph(data, {
    cellSize: 15,
    cellGap: 4,
    showMonthLabels: false,
    showDayLabels: false,
    baseColor: '#00ff00',
    textColor: '#ff00ff',
    cellBackground: '#0000ff',
  });

  assert(svg.includes('<svg'));
  assert(svg.includes('</svg>'));
  // Should not have month labels
  assert(!svg.includes('jan'));
  // Should have fewer text elements without labels
  const textCount = countElements(svg, 'text');
  assertEquals(textCount, 0);
});

Deno.test('generateContributionGraph produces no background rect', () => {
  const data: DailyCommandCount[] = [{date: '2024-01-01', count: 10}];

  const svg = generateContributionGraph(data);

  // Count all rects in the SVG
  const allRects = svg.match(/<rect[^>]*>/g) || [];

  // Check that none of the rects are full-width/height background rects
  // Cell rects should have rx="2" attribute
  const cellRects = allRects.filter(rect => rect.includes('rx="2"'));

  // All rects should be cell rects
  assertEquals(allRects.length, cellRects.length);
});

Deno.test('generateContributionGraph skips partial first month label', () => {
  // Start data on Jan 15 (mid-month), which should skip the Jan label
  const data: DailyCommandCount[] = [
    {date: '2024-01-15', count: 10},
    {date: '2024-01-16', count: 20},
    {date: '2024-02-01', count: 15},
    {date: '2024-02-02', count: 25},
  ];

  const svg = generateContributionGraph(data, {showMonthLabels: true});

  // Should NOT have 'jan' label since we start mid-month
  assert(!svg.includes('jan'));
  // Should have 'feb' label since that month change is included
  assert(svg.includes('feb'));
});

Deno.test('generateContributionGraph shows first month label when starting early in month', () => {
  // Start data on Jan 3 (early in month), which should show the Jan label
  const data: DailyCommandCount[] = [
    {date: '2024-01-03', count: 10},
    {date: '2024-01-04', count: 20},
    {date: '2024-02-01', count: 15},
  ];

  const svg = generateContributionGraph(data, {showMonthLabels: true});

  // Should have 'jan' label since we start early in the month (day <= 7)
  assert(svg.includes('jan'));
  // Should also have 'feb' label
  assert(svg.includes('feb'));
});
