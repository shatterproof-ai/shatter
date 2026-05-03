// NoTargetReason::PolicyExcluded — placeholder file. The broad-run gate
// invokes shatter with `--exclude '**/policy_excluded.ts'` so this entry
// is classified `policy_excluded`.
export function shouldNotBeScanned(value: number): number {
  return value * 2;
}
