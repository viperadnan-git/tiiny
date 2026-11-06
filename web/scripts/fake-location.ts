// Imported first: origin.ts reads the origin at load.
(globalThis as { location?: unknown }).location = { origin: 'https://t.test', host: 't.test' };
