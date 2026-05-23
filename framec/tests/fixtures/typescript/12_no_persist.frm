@@[persist(string)]
@@[save(snapshot)]
@@[load(restore)]
@@system NoPersist {
    interface:
        bump()
        set_cache(v: number)
        get_count(): number
        get_cache(): number

    machine:
        $Active {
            bump() { self.count = self.count + 1 }
            set_cache(v: number) { self.cache = v }
            get_count(): number { @@:(self.count) }
            get_cache(): number { @@:(self.cache) }
        }

    domain:
        count: number = 0
        @@[no_persist]
        cache: number = -1
}
