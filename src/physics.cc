#include "physics.h"

//! The static Environ instance

Environ* Environ::mInstance = 0;

//! Defines the maximum number of contact points we will consider

const unsigned int MAX_CONTACTS = 5;

void NearCallback(void* data, dGeomID g1, dGeomID g2)
{
    // Contact point information
    static dContact contact[MAX_CONTACTS];
    // Our environment
    static Environ* environ = Environ::getInstance();
    // just a counter used in the loops
    static unsigned int i, numContacts;
    // declare whether object a or b should bounce
    static bool bounce_a, bounce_b;

    // ----------------------------------------------------------------
    //
    // INITIALIZATION
    //
    // ----------------------------------------------------------------

    // get the body information associated with each geometry
    dBodyID b1 = dGeomGetBody(g1), b2 = dGeomGetBody(g2);

    // if we already have a contact joint, we can bail out here
    if (b1 && b2 && dAreConnected(b1, b2))
        return;

    // get the CollisionData object from the ODE geometry
    CollisionData *a = (CollisionData*)dGeomGetData(g1),
                  *b = (CollisionData*)dGeomGetData(g2);

    // try to get the Collidable instance from the CollisionData
    Collidable* ca = (a ? ((Collidable*)a->ptr) : 0);
    Collidable* cb = (b ? ((Collidable*)b->ptr) : 0);

    // ----------------------------------------------------------------
    //
    // RAY COLLISION
    //
    // ----------------------------------------------------------------

    // for Ray objects, we can perform avoidance and bailout early

    if ((a && a->type == COLLISION_RAY) || (b && b->type == COLLISION_RAY)) {
        if ((ca && cb) && (ca != cb) && (a->type != b->type)) {
            ca->avoid(cb), cb->avoid(ca);
        }

        return;
    }

    // ----------------------------------------------------------------
    //
    // BOUNCE DETERMINATION
    //
    // ----------------------------------------------------------------

    // initalize to no bouncing
    bounce_a = false, bounce_b = false;

    // try to call Collidable::shouldCollide() on the object
    // if it is valid, otherwise we will ensure it bounces

    bounce_a = (ca ? (ca->shouldCollide(cb)) : true);
    bounce_b = (cb ? (cb->shouldCollide(ca)) : true);

    // if there is no bouncing to be done then bailout

    if (!bounce_a && !bounce_b) {
        return;
    }

    // ----------------------------------------------------------------
    //
    // COLLISION DETECTION
    //
    // ----------------------------------------------------------------

    // initialize the contacts for bounciness
    for (i = 0; i < MAX_CONTACTS; ++i) {
        contact[i].surface.mode = dContactBounce | dContactApprox1;
        contact[i].surface.mu = dInfinity;
        contact[i].surface.bounce = 1;
        contact[i].surface.bounce_vel = .01;
    }

    // dCollide will find all the contact points between the two
    // objects if they exists

    numContacts = dCollide(g1, g2, MAX_CONTACTS,
                           &contact[0].geom,
                           sizeof(dContact));

    // bail out if there aren't any contacts

    if (numContacts == 0) {
        return;
    }

    // ----------------------------------------------------------------
    //
    // COLLISION RESPONSE
    //
    // ----------------------------------------------------------------

    // for each contact, create a contact joint (this is what
    // performs the collision response in the next dWorldStep

    for (i = 0; i < numContacts; ++i) {
        dJointID c = dJointCreateContact(environ->mWorld,
                                         environ->mContactGroup,
                                         contact + i);

        if (bounce_a && bounce_b) {
            dJointAttach(c, b1, b2);
        } else if (bounce_a) {
            dJointAttach(c, b1, 0);
        } else {
            dJointAttach(c, 0, b2);
        }
    }
}
