class Cache {
    get(k: string): string { return "v:" + k; }
}

@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(key: string): string

    machine:
        $Ready {
            fetch(key: string): string {
                @@:(@@:self.cache.get(key))
            }
        }

    domain:
        cache: Cache = null
}
