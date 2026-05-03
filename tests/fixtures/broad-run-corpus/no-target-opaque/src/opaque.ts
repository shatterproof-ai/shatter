// Every function here takes only opaque handle parameters with no
// primitive fields to vary. Shatter should classify the file as a
// no-target file and report each function under skipped / unsupported
// with a clear reason — not silently drop them from the denominator.

interface Connection {
    send(payload: unknown): Promise<void>;
    close(): Promise<void>;
}

interface Stream {
    next(): Promise<unknown>;
    cancel(): void;
}

export async function pipe(conn: Connection): Promise<void> {
    await conn.send({});
    await conn.close();
}

export async function drain(stream: Stream): Promise<void> {
    await stream.next();
    stream.cancel();
}
