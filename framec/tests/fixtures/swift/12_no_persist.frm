@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system NoPersist {
    interface:
        bump()
        set_cache(v: Int)
        get_count(): Int
        get_cache(): Int

    machine:
        $Active {
            bump() { @@:self.count = @@:self.count + 1 }
            set_cache(v: Int) { @@:self.cache = v }
            get_count(): Int { @@:(@@:self.count) }
            get_cache(): Int { @@:(@@:self.cache) }
        }

    domain:
        count: Int = 0
        @@[no_persist]
        cache: Int = -1
}
