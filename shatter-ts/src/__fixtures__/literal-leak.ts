// Fixture for str-jeen.82: literal extraction must not leak strings from
// unrelated module-level object literals whose method bodies contain strings.

const MAX_RETRIES = 3;

export const pickpackitApi = {
  list: () => "/api/workspaces",
  create: () => "POST",
  remove: () => "DELETE",
  update: () => "PATCH",
  err: () => "stringify",
};

export function tagsQueryKey(id: string): readonly [string, string] {
  return ["tags", id];
}

export function usesRetries(x: number): number {
  return x * MAX_RETRIES;
}
