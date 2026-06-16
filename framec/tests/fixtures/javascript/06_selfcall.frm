@@system SelfCall {
    interface:
        kick()
        report(): number

    machine:
        $Active {
            kick() {
                @@:self.count = @@:self.count + 1
                @@:self.report()
            }
            report(): number { @@:(@@:self.count) }
        }

    domain:
        count: number = 0
}
