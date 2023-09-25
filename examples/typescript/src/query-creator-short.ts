const escapeAndQuote = (value: string) => `'${value.replaceAll(/[']/g, "''")}'`
const escapeAndQuoteAll = (values: string[]) => values.map(escapeAndQuote)

export function constructSearchQuery() {
    console.log("yes");
}