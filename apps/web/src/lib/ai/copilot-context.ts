export async function settleList<T>(request: Promise<{ data: T[] }>): Promise<T[]> {
	try {
		return (await request).data;
	} catch {
		return [];
	}
}
