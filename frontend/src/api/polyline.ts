/**
 * Decode a Google Polyline-encoded string into an array of [lat, lon] pairs.
 * @param encoded - the encoded polyline string
 * @param precision - number of decimal places used during encoding (default 5)
 */
export function decodePolyline(
  encoded: string,
  precision = 5,
): [number, number][] {
  const factor = 10 ** precision;
  const result: [number, number][] = [];
  let index = 0;
  let lat = 0;
  let lon = 0;

  while (index < encoded.length) {
    let shift = 0;
    let value = 0;
    let b: number;
    do {
      b = encoded.charCodeAt(index++) - 63;
      value |= (b & 0x1f) << shift;
      shift += 5;
    } while (b >= 0x20);
    lat += value & 1 ? ~(value >> 1) : value >> 1;

    shift = 0;
    value = 0;
    do {
      b = encoded.charCodeAt(index++) - 63;
      value |= (b & 0x1f) << shift;
      shift += 5;
    } while (b >= 0x20);
    lon += value & 1 ? ~(value >> 1) : value >> 1;

    result.push([lat / factor, lon / factor]);
  }

  return result;
}
