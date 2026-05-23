@@system Lifecycle {
    interface:
        start(label: std::string)
        stop()

    machine:
        $Idle {
            start(label: std::string) {
                (label)
                -> $Running
            }
        }

        $Running {
            $>(label: std::string) {
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
        entered: int = 0
        exited: int = 0
        tag: std::string = ""
}
