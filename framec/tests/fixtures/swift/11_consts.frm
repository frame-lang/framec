@@system Consts(step: Int = 5, limit: Int = 20) {
    interface:
        tick()
        get_count(): Int

    machine:
        $Running {
            tick() {
                self.count = self.count + self.step;
                if self.count >= self.limit {
                    self.count = 0;
                }
            }
            get_count(): Int { @@:(self.count) }
        }

    domain:
        step: Int = 5
        limit: Int = 20
        count: Int = 0
}
