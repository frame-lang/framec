@@system Consts(step: number = 5, limit: number = 20) {
    interface:
        tick()
        get_count(): number

    machine:
        $Running {
            tick() {
                @@:self.count = @@:self.count + @@:self.step;
                if @@:self.count >= @@:self.limit then
                    @@:self.count = 0;
                end
            }
            get_count(): number { @@:(@@:self.count) }
        }

    domain:
        step: number = 5
        limit: number = 20
        count: number = 0
}
