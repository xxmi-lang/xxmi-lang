/** "Did you mean …" suggestions by edit distance. */

export function editDistance(a: string, b: string): number {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  let prev = Array.from({ length: y.length + 1 }, (_, i) => i);
  for (let i = 1; i <= x.length; i++) {
    const row = [i];
    for (let j = 1; j <= y.length; j++) {
      const cost = x[i - 1] === y[j - 1] ? 0 : 1;
      row[j] = Math.min((prev[j] ?? 0) + 1, (row[j - 1] ?? 0) + 1, (prev[j - 1] ?? 0) + cost);
    }
    prev = row;
  }
  return prev[y.length] ?? 0;
}

/** The closest candidate within a distance that scales with length, or undefined. */
export function nearest(word: string, candidates: Iterable<string>): string | undefined {
  const limit = Math.max(1, Math.min(3, Math.floor(word.length / 3)));
  let best: string | undefined;
  let bestDistance = limit + 1;
  for (const candidate of candidates) {
    const d = editDistance(word, candidate);
    if (d < bestDistance || (d === bestDistance && best !== undefined && candidate < best)) {
      best = candidate;
      bestDistance = d;
    }
  }
  return bestDistance <= limit ? best : undefined;
}

export function didYouMean(word: string, candidates: Iterable<string>): string {
  const match = nearest(word, candidates);
  return match ? ` Did you mean \`${match}\`?` : '';
}
