@@system Consts(step: integer = 5, limit: integer = 20) {
    interface:
        tick()
        get_count(): integer

    machine:
        $Running {
            tick() {
                self.count = self.count + self.step;
                if self.count >= self.limit {
                    self.count = 0;
                }
            }
            get_count(): integer { @@:(self.count) }
        }

    domain:
        step: integer = 5
        limit: integer = 20
        count: integer = 0
}
