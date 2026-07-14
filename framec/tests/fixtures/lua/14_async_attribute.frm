local Cache = {}
function Cache:get(k) return "v:" .. k end

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
        cache: Cache = nil
}
