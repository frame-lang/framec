@@system Consts(step: Integer = 5, limit: Integer = 20) {
    interface:
        tick()
        get_count(): Integer

    machine:
        $Running {
            tick() {
                @@:self.count = @@:self.count + @@:self.step;
                if @@:self.count >= @@:self.limit
                    @@:self.count = 0;
                end
            }
            get_count(): Integer { @@:(@@:self.count) }
        }

    domain:
        step: Integer = 5
        limit: Integer = 20
        count: Integer = 0
}
