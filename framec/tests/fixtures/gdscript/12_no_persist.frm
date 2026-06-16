@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system NoPersist {
    interface:
        bump()
        set_cache(v: int)
        get_count(): int
        get_cache(): int

    machine:
        $Active {
            bump() { @@:self.count = @@:self.count + 1 }
            set_cache(v: int) { @@:self.cache = v }
            get_count(): int { @@:(@@:self.count) }
            get_cache(): int { @@:(@@:self.cache) }
        }

    domain:
        count: int = 0
        @@[no_persist]
        cache: int = -1
}
