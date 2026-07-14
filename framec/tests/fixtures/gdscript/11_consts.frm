@@system Consts(step: int = 5, limit: int = 20) {
    interface:
        tick()
        get_count(): int

    machine:
        $Running {
            tick() {
                @@:self.count = @@:self.count + @@:self.step;
                if @@:self.count >= @@:self.limit:
                    @@:self.count = 0;
            }
            get_count(): int { @@:(@@:self.count) }
        }

    domain:
        step: int = 5
        limit: int = 20
        count: int = 0
}
