@@system Lifecycle {
    interface:
        start(label: String)
        stop()

    machine:
        $Idle {
            start(label: String) {
                (label)
                -> $Running
            }
        }

        $Running {
            $>(label: String) {
                self.entered = self.entered + 1;
                self.tag = label;
            }
            <$() {
                self.exited = self.exited + 1
            }
            stop() {
                -> $Idle
            }
        }

    domain:
        entered: Int = 0
        exited: Int = 0
        tag: String = ""
}
