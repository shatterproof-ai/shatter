/**
 * Fixture for medium-confidence opaque type detection tests.
 *
 * These types are defined in this file (not in node_modules) so only
 * heuristics 2 (closeable_interface) and 3 (native_handle_field) apply.
 * Heuristic 1 (infrastructure_package) requires types from known npm packages.
 */

// closeable_interface: has a close() method
export class ResourceHandle {
  public id: number = 0;
  close(): void {}
}
export function handleResource(r: ResourceHandle): void { void r; }

// native_handle_field: has an fd field
export class FdWrapper {
  public fd: number = -1;
  public name: string = "";
}
export function handleFd(w: FdWrapper): void { void w; }

// native_handle_field: has a handle field
export class OsHandle {
  public handle: number = 0;
}
export function handleOs(h: OsHandle): void { void h; }

// NOT opaque: plain data class with no close method and no handle fields
export class SafeData {
  public name: string = "";
  public value: number = 0;
}
export function handleSafe(d: SafeData): void { void d; }
