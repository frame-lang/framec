class Cache
  def get(k) = "v:" + k
end

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
