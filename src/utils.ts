/**
 * Parse period string (e.g., "1y", "6m", "30d") into start date
 * Returns the start date in YYYY-MM-DD format, with end date being today
 */
export function parsePeriod(
  periodStr: string,
  timezone: string
): {startDate: string; endDate: string} | null {
  const match = periodStr.match(/^(\d+)([ymd])$/i);
  if (!match) {
    return null;
  }

  const value = parseInt(match[1], 10);
  const unit = match[2].toLowerCase();

  if (value <= 0 || isNaN(value)) {
    return null;
  }

  // Get today's date in the specified timezone
  const now = Temporal.Now.zonedDateTimeISO(timezone);
  const endDate = now.toPlainDate();

  let startDate: Temporal.PlainDate;

  switch (unit) {
    case 'y':
      startDate = endDate.subtract({years: value});
      break;
    case 'm':
      startDate = endDate.subtract({months: value});
      break;
    case 'd':
      startDate = endDate.subtract({days: value});
      break;
    default:
      return null;
  }

  return {
    startDate: startDate.toString(),
    endDate: endDate.toString(),
  };
}
