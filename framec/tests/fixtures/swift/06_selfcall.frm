@@system SelfCall {
    interface:
        kick()
        report(): Int

    machine:
        $Active {
            kick() {
                @@:self.count = @@:self.count + 1
                @@:self.report()
            }
            report(): Int { @@:(@@:self.count) }
        }

    domain:
        count: Int = 0
}
