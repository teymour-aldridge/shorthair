use db::user::User;

use crate::{html::page_of_body, request_ids::TracingSpan};

#[get("/")]
pub fn index(user: Option<User>, span: TracingSpan) -> maud::Markup {
    let _guard = span.0.enter();
    page_of_body(
        maud::html! {
            div.container {
                div.row.justify-content-center.my-5 {
                    div.col-md-8.text-center {
                        h1.display-4 { "Simple spar generation" }
                        p.lead { "Automatically manage signups, generate draws and collect results." }
                        div.mt-4 {
                            a href="/how_it_works" { "How it works" }
                            span.text-muted.ml-2.small { " (Note: the how it
                                works page loads ~2MB of images, so you may not
                                want to visit if you are on a metered internet
                                connection.)" }
                        }
                    }
                }
            }
        },
        user,
    )
}

#[get("/how_it_works")]
pub fn how_it_works(user: Option<User>, span: TracingSpan) -> maud::Markup {
    let _guard = span.0.enter();
    page_of_body(
        maud::html! {
            div.container {
                h1.text-center.my-4 { "How it works" }
                p.text-muted.text-center.mb-5 { "Note: these screenshots come
                    from a past real-life spar, so the names of participants
                    have been redacted (with black highlight)." }

                div.row.mb-5 {
                    div.col-lg-8.mx-auto {
                        p.lead { "Create a new spar (participants can then sign
                                  up using the QR code - they don't need to have
                                  an account, they just input their name and
                                  email address)." }
                        img.img-fluid.rounded.shadow-sm.mb-4.w-100 src="https://eldemite-public.t3.storage.dev/step1.png" alt="Step 1" {}
                    }
                }

                div.row.mb-5 {
                    div.col-lg-8.mx-auto {
                        p.lead { "Then click 'generate draw' to produce a new
                                  draw." }
                        img.img-fluid.rounded.shadow-sm.mb-4.w-100 src="https://eldemite-public.t3.storage.dev/step2.png" alt="Step 2" {}
                    }
                }

                div.row.mb-5 {
                    div.col-lg-8.mx-auto {
                        p.lead { "Wait for the draw to be generated (in my
                                  experience this takes around 15 seconds for a
                                  3 room spar)" }
                        img.img-fluid.rounded.shadow-sm.mb-4.w-100 src="https://eldemite-public.t3.storage.dev/step3.png" alt="Step 3" {}
                    }
                }

                div.row.mb-5 {
                    div.col-lg-8.mx-auto {
                        p.lead { "Review the draw, and then confirm it.
                                  (Note: support for editing draws before they
                                   are released is planned)." }
                        img.img-fluid.rounded.shadow-sm.mb-4.w-100 src="https://eldemite-public.t3.storage.dev/step4.png" alt="Step 4" {}
                    }
                }

                div.row.mb-5 {
                    div.col-lg-8.mx-auto {
                        p.lead { "Click confirm to proceed (you will get a
                                  warning if you have already set a draw)." }
                        img.img-fluid.rounded.shadow-sm.mb-4.w-100 src="https://eldemite-public.t3.storage.dev/step5.png" alt="Step 5" {}
                    }
                }

                div.row.mb-5 {
                    div.col-lg-8.mx-auto {
                        p.lead { "Click the 'release draw' button to make the
                                  draw public." }
                        img.img-fluid.rounded.shadow-sm.mb-4.w-100 src="https://eldemite-public.t3.storage.dev/step6.png" alt="Step 6" {}
                    }
                }

                div.row.mb-5 {
                    div.col-lg-8.mx-auto {
                        p.lead { "Participants can now view the draw on the
                                  public spar page." }
                        img.img-fluid.rounded.shadow-sm.mb-4.w-100 src="https://eldemite-public.t3.storage.dev/step7.png" alt="Step 7" {}
                    }
                }

                div.row.mb-5 {
                    div.col-lg-8.mx-auto {
                        p.lead { "Adjudicators will be emailed a link which
                                  takes them to a page where they can fill out
                                  ballots. Ballot information is used when
                                  allocating participants (to compute the
                                  relative strengths of different participants
                                  and allocate them to evenly-matched rooms)." }
                        img.img-fluid.rounded.shadow-sm.mb-4.w-100 src="https://eldemite-public.t3.storage.dev/step8.png" alt="Step 8" {}
                    }
                }


            }
        },
        user,
    )
}
