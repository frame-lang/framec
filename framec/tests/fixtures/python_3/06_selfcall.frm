@@system SelfCall {
    interface:
        kick()
        report(): int

    machine:
        $Active {
            kick() {
                @@:self.count = @@:self.count + 1
                @@:self.report()
            }
            report(): int { @@:(@@:self.count) }
        }

    domain:
        count: int = 0
}
