@@system KeywordIdent {
    interface:
        init(default: Int)
        run()

    machine:
        $S {
            init(default: Int) { @@:self.guard = default }
            run() { @@:self.init(7) }
        }

    domain:
        guard: Int = 0
}
