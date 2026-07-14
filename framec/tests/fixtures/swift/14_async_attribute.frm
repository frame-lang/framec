class Cache {
    func get(_ k: String) -> String { return "v:" + k }
}

@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(key: String): String

    machine:
        $Ready {
            fetch(key: String): String {
                @@:(@@:self.cache.get(key))
            }
        }

    domain:
        cache: Cache = nil
}
